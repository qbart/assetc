#![doc = include_str!("../README.md")]
//!
//! Start with [`Assets::open`] on a `.pak` file or an output directory, then
//! [`Assets::load`] any asset by ID into one of the format types.

pub mod anim;
pub mod assets;
pub mod atlas;
pub mod chunks;
pub mod custom_material;
pub mod data;
mod error;
pub mod font;
pub mod gradient;
pub mod graph;
pub mod hash;
pub mod manifest;
pub mod material;
pub mod mesh;
pub mod pack;
pub mod palette;
pub mod property;
mod read;
pub mod schema;
pub mod shader;
pub mod structs;
pub mod texture;
pub mod tree;

pub use anim::Animation;
pub use assets::{Assets, Decode, Entry};
pub use atlas::Atlas;
pub use custom_material::CustomMaterial;
pub use data::Data;
pub use error::{Error, Result};
pub use font::Font;
pub use gradient::GradientSet;
pub use graph::Graph;
pub use manifest::Manifest;
pub use material::Material;
pub use mesh::Mesh;
pub use pack::Pack;
pub use palette::Palette;
pub use property::{Property, PropertyValue};
pub use schema::Schema;
pub use shader::Shader;
pub use structs::{Attached, Scope, Structs};
pub use texture::Texture;
pub use tree::Tree;

/// What an asset is, with the values of the manifest and `.pak` contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Texture,
    Material,
    Mesh,
    Animation,
    Font,
    Schema,
    Data,
    Graph,
    Tree,
    Atlas,
    /// `.agradient.bin`.
    Gradient,
    /// `.apalette.bin`.
    Palette,
    /// `.ashader.bin`.
    Shader,
    /// `.acmat.bin`.
    CustomMaterial,
    /// `assetc.aman.bin` (packs only).
    Manifest,
    Other,
}

impl Kind {
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Texture,
            1 => Self::Material,
            2 => Self::Mesh,
            3 => Self::Animation,
            4 => Self::Font,
            5 => Self::Schema,
            6 => Self::Data,
            7 => Self::Graph,
            8 => Self::Tree,
            9 => Self::Atlas,
            10 => Self::Manifest,
            11 => Self::Gradient,
            12 => Self::Palette,
            13 => Self::Shader,
            14 => Self::CustomMaterial,
            _ => Self::Other,
        }
    }

    /// The kind of an output file, by its name.
    pub fn of_path(path: &str) -> Self {
        let name = path.rsplit('/').next().unwrap_or(path);
        if name == manifest::FILE_NAME {
            return Self::Manifest;
        }
        let lower = name.to_ascii_lowercase();
        [
            (".ktx2", Self::Texture),
            (".amat.bin", Self::Material),
            (".amesh.bin", Self::Mesh),
            (".aanim.bin", Self::Animation),
            (".afont.bin", Self::Font),
            (".aschema.bin", Self::Schema),
            (".adata.bin", Self::Data),
            (".agraph.bin", Self::Graph),
            (".atree.bin", Self::Tree),
            (".aatlas.bin", Self::Atlas),
            (".agradient.bin", Self::Gradient),
            (".apalette.bin", Self::Palette),
            (".ashader.bin", Self::Shader),
            (".acmat.bin", Self::CustomMaterial),
        ]
        .into_iter()
        .find(|(ext, _)| lower.ends_with(ext))
        .map_or(Self::Other, |(_, kind)| kind)
    }
}

#[cfg(test)]
mod tests {
    use super::Kind;

    #[test]
    fn kinds_by_value_and_path() {
        assert_eq!(Kind::from_u8(11), Kind::Gradient);
        assert_eq!(Kind::from_u8(12), Kind::Palette);
        assert_eq!(Kind::from_u8(13), Kind::Shader);
        assert_eq!(Kind::from_u8(14), Kind::CustomMaterial);
        assert_eq!(Kind::from_u8(15), Kind::Other);
        assert_eq!(Kind::of_path("shaders/water.ashader.bin"), Kind::Shader);
        assert_eq!(Kind::of_path("water/lake.acmat.bin"), Kind::CustomMaterial);
        assert_eq!(Kind::of_path("water/lake.amat.bin"), Kind::Material);
        assert_eq!(Kind::of_path("ui/fire.agradient.bin"), Kind::Gradient);
        assert_eq!(Kind::of_path("ui/Base.apalette.bin"), Kind::Palette);
        assert_eq!(Kind::of_path("ui/fire.ktx2"), Kind::Texture);
    }
}
