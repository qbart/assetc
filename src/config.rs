use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use serde::Deserialize;

use crate::amesh::{MeshOptions, MeshPatch};

// See docs/spec/assetc.toml.md.
pub const FILE_NAME: &str = "assetc.toml";

const TEMPLATE: &str = "\
# assetc project configuration.
# See docs/spec/assetc.toml.md for available options.

input = \"assets\"
output = \"build\"

# [textures]
# linear = [\"**/*_n.png\"]
# no_mipmaps = [\"ui/**\"]

# [meshes]
# meshlet_max_vertices = 64
# meshlet_max_triangles = 124
#
# [[meshes.overrides]]
# match = [\"characters/**\"]
# meshlet_max_vertices = 128
# meshlet_max_triangles = 256
";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Source assets directory, relative to the project root.
    pub input: PathBuf,
    /// Compiled assets directory, relative to the project root.
    pub output: PathBuf,
    pub textures: Textures,
    pub meshes: Meshes,
}

/// Per-texture settings, see docs/features/textures.md.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Textures {
    /// Globs of textures holding linear data instead of sRGB color.
    pub linear: Vec<String>,
    /// Globs of textures written without mipmaps.
    pub no_mipmaps: Vec<String>,
    /// Globs of color LUTs compiled as 3D textures instead of 2D strips.
    pub volume_luts: Vec<String>,
}

/// Mesh build settings, see docs/spec/assetc.toml.md#meshes.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(try_from = "toml::Table")]
pub struct Meshes {
    pub defaults: MeshOptions,
    /// Per-model values; the last matching one setting a value wins.
    pub overrides: Vec<MeshOverride>,
}

impl TryFrom<toml::Table> for Meshes {
    type Error = String;

    fn try_from(mut table: toml::Table) -> Result<Self, String> {
        let overrides = match table.remove("overrides") {
            None => Vec::new(),
            Some(value) => value
                .try_into()
                .map_err(|e: toml::de::Error| format!("overrides: {}", e.message()))?,
        };
        let defaults = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        Ok(Self {
            defaults,
            overrides,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(try_from = "toml::Table")]
pub struct MeshOverride {
    /// Globs of model paths, as in `[textures]`.
    pub patterns: Vec<String>,
    pub patch: MeshPatch,
}

impl TryFrom<toml::Table> for MeshOverride {
    type Error = String;

    fn try_from(mut table: toml::Table) -> Result<Self, String> {
        let patterns = table
            .remove("match")
            .ok_or("missing match")?
            .try_into()
            .map_err(|e: toml::de::Error| format!("match: {}", e.message()))?;
        let patch = toml::Value::Table(table)
            .try_into()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        Ok(Self { patterns, patch })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input: "assets".into(),
            output: "build".into(),
            textures: Textures::default(),
            meshes: Meshes::default(),
        }
    }
}

/// The loaded project: its root (the directory holding `assetc.toml`) and config.
#[derive(Debug, Clone, Default, Resource)]
pub struct Project {
    pub root: Option<PathBuf>,
    pub config: Config,
}

impl Project {
    pub fn input_dir(&self) -> Option<PathBuf> {
        Some(self.root.as_ref()?.join(&self.config.input))
    }

    pub fn output_dir(&self) -> Option<PathBuf> {
        Some(self.root.as_ref()?.join(&self.config.output))
    }
}

#[derive(Debug)]
pub enum Error {
    Exists(PathBuf),
    Io(PathBuf, io::Error),
    Parse(PathBuf, toml::de::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists(path) => write!(f, "{} already exists", path.display()),
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Self::Parse(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// Walks up from `start` and returns the first `assetc.toml` found.
pub fn find(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(FILE_NAME))
        .find(|path| path.is_file())
}

/// Discovers and loads the project config; no config found yields the default project.
pub fn load(start: &Path) -> Result<Project, Error> {
    let Some(path) = find(start) else {
        return Ok(Project::default());
    };
    let text = fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
    let config = toml::from_str(&text).map_err(|e| Error::Parse(path.clone(), e))?;
    Ok(Project {
        root: path.parent().map(Path::to_path_buf),
        config,
    })
}

/// Creates `assetc.toml` in `dir`, never overwriting an existing one.
pub fn init(dir: &Path) -> Result<PathBuf, Error> {
    let path = dir.join(FILE_NAME);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| match e.kind() {
            io::ErrorKind::AlreadyExists => Error::Exists(path.clone()),
            _ => Error::Io(path.clone(), e),
        })?;
    file.write_all(TEMPLATE.as_bytes())
        .map_err(|e| Error::Io(path.clone(), e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_writes_template() {
        let dir = tempfile::tempdir().unwrap();
        let path = init(dir.path()).unwrap();
        assert_eq!(path, dir.path().join(FILE_NAME));
        assert_eq!(fs::read_to_string(&path).unwrap(), TEMPLATE);
    }

    #[test]
    fn init_refuses_to_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "# keep\n").unwrap();
        assert!(matches!(init(dir.path()), Err(Error::Exists(_))));
        assert_eq!(
            fs::read_to_string(dir.path().join(FILE_NAME)).unwrap(),
            "# keep\n"
        );
    }

    #[test]
    fn template_parses_as_default_config() {
        let config: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn empty_file_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn dirs_resolve_against_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "input = \"src\"\n").unwrap();
        let project = load(dir.path()).unwrap();
        assert_eq!(project.input_dir(), Some(dir.path().join("src")));
        assert_eq!(project.output_dir(), Some(dir.path().join("build")));
    }

    #[test]
    fn dirs_without_project_are_none() {
        let project = Project::default();
        assert_eq!(project.input_dir(), None);
        assert_eq!(project.output_dir(), None);
    }

    #[test]
    fn load_finds_config_in_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path()).unwrap();
        let nested = dir.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();

        let project = load(&nested).unwrap();
        assert_eq!(project.root.as_deref(), Some(dir.path()));
    }

    #[test]
    fn load_prefers_nearest_config() {
        let dir = tempfile::tempdir().unwrap();
        init(dir.path()).unwrap();
        let inner = dir.path().join("inner");
        fs::create_dir_all(inner.join("deep")).unwrap();
        init(&inner).unwrap();

        let project = load(&inner.join("deep")).unwrap();
        assert_eq!(project.root.as_deref(), Some(inner.as_path()));
    }

    #[test]
    fn load_without_config_is_default() {
        let dir = tempfile::tempdir().unwrap();
        // Only meaningful if no assetc.toml exists above the temp dir.
        if find(dir.path()).is_none() {
            let project = load(dir.path()).unwrap();
            assert!(project.root.is_none());
        }
    }

    #[test]
    fn parses_texture_settings() {
        let config: Config =
            toml::from_str("[textures]\nlinear = [\"**/*_n.png\"]\nno_mipmaps = [\"ui/**\"]\n")
                .unwrap();
        assert_eq!(config.textures.linear, ["**/*_n.png"]);
        assert_eq!(config.textures.no_mipmaps, ["ui/**"]);
        assert!(toml::from_str::<Config>("[textures]\nlinaer = []\n").is_err());
    }

    #[test]
    fn parses_mesh_settings() {
        let config: Config = toml::from_str(
            "[meshes]\nmeshlet_max_vertices = 96\n\n[[meshes.overrides]]\nmatch = [\"a/**\"]\nmeshlet_max_triangles = 64\n",
        )
        .unwrap();
        assert_eq!(config.meshes.defaults.meshlet_max_vertices, 96);
        assert_eq!(config.meshes.defaults.meshlet_max_triangles, 124);
        assert_eq!(config.meshes.overrides[0].patterns, ["a/**"]);
        assert_eq!(config.meshes.overrides[0].patch.meshlet_max_triangles, Some(64));
        assert!(
            toml::from_str::<Config>("[[meshes.overrides]]\nmeshlet_max_vertices = 8\n").is_err()
        );
        assert!(toml::from_str::<Config>("[meshes]\nmax = 1\n").is_err());
        // The template's commented settings parse once uncommented.
        let uncommented: String = TEMPLATE
            .lines()
            .map(|l| l.strip_prefix("# ").unwrap_or(l))
            .filter(|l| !l.starts_with("assetc") && !l.starts_with("See") && *l != "#")
            .collect::<Vec<_>>()
            .join("\n");
        let template: Config = toml::from_str(&uncommented).unwrap();
        assert_eq!(template.meshes.overrides[0].patch.meshlet_max_vertices, Some(128));
    }

    #[test]
    fn load_rejects_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "typo = 1\n").unwrap();
        assert!(matches!(load(dir.path()), Err(Error::Parse(..))));
    }
}
