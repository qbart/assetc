//! One way in to a pack or an output directory; see docs/features/sdk.md#opening-assets.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::manifest::{self, Manifest};
use crate::pack::Pack;
use crate::{Error, Kind, Result};

/// An asset the assets hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    /// Relative to the output directory, `/`-separated.
    pub path: String,
}

/// A format type [`Assets::load`] can decode.
pub trait Decode: Sized {
    fn decode(bytes: &[u8]) -> Result<Self>;
}

macro_rules! decode {
    ($($ty:ty),*) => {
        $(impl Decode for $ty {
            fn decode(bytes: &[u8]) -> Result<Self> {
                <$ty>::decode(bytes)
            }
        })*
    };
}

decode!(
    crate::Texture,
    crate::Material,
    crate::Manifest,
    crate::Mesh,
    crate::Animation,
    crate::Font,
    crate::Atlas,
    crate::Schema,
    crate::Data,
    crate::Graph,
    crate::Tree,
    crate::GradientSet,
    crate::Palette,
    crate::Shader,
    crate::CustomMaterial
);

enum Source {
    Pack(Pack),
    Dir(PathBuf),
}

/// A pack or an output directory, read the same way.
pub struct Assets {
    source: Source,
    /// Sorted by path.
    entries: Vec<Entry>,
    by_id: HashMap<u64, usize>,
}

impl Assets {
    /// Opens a `.pak` file or an output directory holding `assetc.aman.bin`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.is_dir() {
            let bytes = std::fs::read(path.join(manifest::FILE_NAME))?;
            let manifest = Manifest::decode(&bytes).map_err(|e| e.in_file(manifest::FILE_NAME))?;
            let entries = manifest
                .entries
                .into_iter()
                .map(|e| Entry {
                    id: e.id,
                    kind: e.kind,
                    path: e.path,
                })
                .collect();
            Ok(Self::new(Source::Dir(path.to_path_buf()), entries))
        } else {
            Self::from_pack(std::fs::read(path)?)
        }
    }

    /// A pack already in memory.
    pub fn from_pack(bytes: Vec<u8>) -> Result<Self> {
        let pack = Pack::decode(bytes)?;
        let entries = pack
            .entries
            .iter()
            .map(|e| Entry {
                id: e.id,
                kind: e.kind,
                path: e.path.clone(),
            })
            .collect();
        Ok(Self::new(Source::Pack(pack), entries))
    }

    fn new(source: Source, mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let by_id = entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        Self {
            source,
            entries,
            by_id,
        }
    }

    /// The pack, when the assets are one.
    pub fn pack(&self) -> Option<&Pack> {
        match &self.source {
            Source::Pack(pack) => Some(pack),
            Source::Dir(_) => None,
        }
    }

    /// Every asset, by path.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The assets of `kind`, by path.
    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }

    pub fn entry(&self, id: u64) -> Option<&Entry> {
        self.by_id.get(&id).map(|&i| &self.entries[i])
    }

    /// The asset at output path `path` (`models/Fox.amesh.bin`).
    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries
            .binary_search_by(|e| e.path.as_str().cmp(path))
            .ok()
            .map(|i| &self.entries[i])
    }

    /// The bytes of asset `id`: borrowed from a pack, read from disk for a directory.
    pub fn read(&self, id: u64) -> Result<Cow<'_, [u8]>> {
        let entry = self
            .entry(id)
            .ok_or_else(|| Error::NotFound(format!("asset {id:#018x}")))?;
        self.read_entry(entry)
    }

    /// The bytes of the file at output path `path`.
    pub fn read_path(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        let entry = self
            .find(path)
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        self.read_entry(entry)
    }

    fn read_entry(&self, entry: &Entry) -> Result<Cow<'_, [u8]>> {
        match &self.source {
            Source::Pack(pack) => {
                let found = pack
                    .find(&entry.path)
                    .ok_or_else(|| Error::NotFound(entry.path.clone()))?;
                Ok(Cow::Borrowed(pack.bytes(found)))
            }
            Source::Dir(dir) => Ok(Cow::Owned(std::fs::read(dir.join(&entry.path))?)),
        }
    }

    /// Reads and decodes asset `id`; errors name its path.
    pub fn load<T: Decode>(&self, id: u64) -> Result<T> {
        let path = self.entry(id).map(|e| e.path.clone()).unwrap_or_default();
        T::decode(&self.read(id)?).map_err(|e| e.in_file(&path))
    }

    /// Reads and decodes the file at output path `path`.
    pub fn load_path<T: Decode>(&self, path: &str) -> Result<T> {
        T::decode(&self.read_path(path)?).map_err(|e| e.in_file(path))
    }
}
