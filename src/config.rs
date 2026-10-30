use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use serde::Deserialize;

// See docs/spec/assetc.toml.md.
pub const FILE_NAME: &str = "assetc.toml";

const TEMPLATE: &str = "\
# assetc project configuration.
# See docs/spec/assetc.toml.md for available options.
";

#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {}

/// The loaded project: its root (the directory holding `assetc.toml`) and config.
#[derive(Debug, Default, Resource)]
pub struct Project {
    pub root: Option<PathBuf>,
    #[expect(dead_code, reason = "config has no keys yet")]
    pub config: Config,
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
    fn load_rejects_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE_NAME), "typo = 1\n").unwrap();
        assert!(matches!(load(dir.path()), Err(Error::Parse(..))));
    }
}
