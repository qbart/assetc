//! The open source projects assetc is built with; see docs/features/gui.md#open-source.

/// A project: name, what assetc uses it for, license, site (without `https://`).
pub struct Project {
    pub name: &'static str,
    pub role: &'static str,
    pub license: &'static str,
    pub site: &'static str,
}

const fn project(
    name: &'static str,
    role: &'static str,
    license: &'static str,
    site: &'static str,
) -> Project {
    Project {
        name,
        role,
        license,
        site,
    }
}

pub const PROJECTS: [Project; 23] = [
    project("Lucide", "icons", "ISC", "lucide.dev"),
    project(
        "Ubuntu",
        "typeface",
        "Ubuntu Font Licence 1.0",
        "design.ubuntu.com/font",
    ),
    project(
        "Hack",
        "monospace typeface",
        "MIT",
        "sourcefoundry.org/hack",
    ),
    project(
        "Noto Emoji",
        "emoji",
        "SIL Open Font License 1.1",
        "fonts.google.com/noto",
    ),
    project(
        "Bevy",
        "application framework",
        "MIT or Apache-2.0",
        "bevy.org",
    ),
    project("wgpu", "graphics", "MIT or Apache-2.0", "wgpu.rs"),
    project(
        "winit",
        "windowing",
        "Apache-2.0",
        "github.com/rust-windowing/winit",
    ),
    project(
        "egui",
        "user interface",
        "MIT or Apache-2.0",
        "github.com/emilk/egui",
    ),
    project(
        "bevy_egui",
        "egui integration",
        "MIT",
        "github.com/vladbat00/bevy_egui",
    ),
    project(
        "resvg",
        "drawing icons",
        "Apache-2.0 or MIT",
        "github.com/linebender/resvg",
    ),
    project(
        "gltf",
        "reading models",
        "MIT or Apache-2.0",
        "github.com/gltf-rs/gltf",
    ),
    project(
        "meshoptimizer",
        "meshlets, levels of detail",
        "MIT",
        "meshoptimizer.org",
    ),
    project(
        "meshopt-pure",
        "meshoptimizer in Rust",
        "MIT",
        "github.com/qbart/meshopt-pure",
    ),
    project("matma", "path curves", "MIT", "github.com/qbart/matma"),
    project(
        "image",
        "reading images",
        "MIT or Apache-2.0",
        "github.com/image-rs/image",
    ),
    project(
        "ttf-parser",
        "reading fonts",
        "MIT or Apache-2.0",
        "github.com/harfbuzz/ttf-parser",
    ),
    project(
        "ab_glyph",
        "font atlases",
        "Apache-2.0",
        "github.com/alexheretic/ab-glyph",
    ),
    project(
        "serde and toml",
        "settings and schema files",
        "MIT or Apache-2.0",
        "serde.rs",
    ),
    project(
        "globset",
        "path patterns",
        "Unlicense or MIT",
        "github.com/BurntSushi/ripgrep",
    ),
    project(
        "clap",
        "command line",
        "MIT or Apache-2.0",
        "github.com/clap-rs/clap",
    ),
    project(
        "base64",
        "embedded glTF data",
        "MIT or Apache-2.0",
        "github.com/marshallpierce/rust-base64",
    ),
    project(
        "webbrowser",
        "opening links",
        "MIT or Apache-2.0",
        "github.com/amodm/webbrowser-rs",
    ),
    project(
        "BLAKE3",
        "hashing in Bevy, pure Rust",
        "CC0-1.0 or Apache-2.0",
        "github.com/BLAKE3-team/BLAKE3",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every dependency in Cargo.toml is credited, by itself or through the project it is
    /// part of.
    #[test]
    fn every_dependency_is_listed() {
        let manifest: toml::Table = include_str!("../Cargo.toml").parse().unwrap();
        fn part_of(name: &str) -> &str {
            match name {
                "egui_extras" => "egui",
                "serde" | "serde_json" | "toml" => "serde and toml",
                other => other,
            }
        }
        for name in manifest["dependencies"].as_table().unwrap().keys() {
            let listed = part_of(name);
            assert!(
                PROJECTS.iter().any(|p| p.name.eq_ignore_ascii_case(listed)),
                "{name} is not in the Open source list"
            );
        }
    }
}
