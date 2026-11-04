//! Where a model's locators come from: its file, presets and its sidecar; see
//! docs/features/locators.md#inherited-locators.

use std::path::Path;

use crate::amod::{self, Locator};
use crate::config;
use crate::model;

/// Where an inherited locator comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The model's glTF.
    File,
    /// A preset, by the input-relative path of its `from` model.
    Preset(String),
}

impl Source {
    /// `file`, or the preset model's file name.
    pub fn label(&self) -> &str {
        match self {
            Self::File => "file",
            Self::Preset(from) => from.rsplit('/').next().unwrap_or(from),
        }
    }
}

/// A locator the model has without its sidecar.
#[derive(Debug, Clone, PartialEq)]
pub struct Inherited {
    pub locator: Locator,
    pub source: Source,
}

/// `over` laid on `base`: a locator with a name already there replaces it in place, others
/// are added at the end.
pub fn lay<T>(base: &mut Vec<T>, over: impl IntoIterator<Item = T>, name: impl Fn(&T) -> &str) {
    for item in over {
        match base.iter().position(|b| name(b) == name(&item)) {
            Some(i) => base[i] = item,
            None => base.push(item),
        }
    }
}

/// The locators the model at `rel` (input-relative) with glTF `data` inherits: its file's,
/// then each matching preset's.
pub fn inherited(
    presets: &config::Locators,
    input: &Path,
    rel: &str,
    data: &[u8],
) -> Result<Vec<Inherited>, String> {
    let mut out: Vec<Inherited> = Vec::new();
    let from_file = model::file_locators(data)?
        .into_iter()
        .map(|locator| Inherited {
            locator,
            source: Source::File,
        });
    lay(&mut out, from_file, |i| &i.locator.name);
    let joints = model::joint_names(data)?;
    for (n, preset) in presets.presets.iter().enumerate() {
        let fail = |e: String| format!("assetc.toml [[locators.presets]] {}: {e}", n + 1);
        let globs = preset
            .patterns
            .as_ref()
            .map(|p| crate::texture::glob_set(p))
            .transpose()
            .map_err(|e| fail(e.to_string()))?;
        let shared = shared(input, &preset.from).map_err(fail)?;
        let applies = preset.from != rel
            && globs.is_none_or(|g| g.is_match(rel))
            && shared
                .iter()
                .all(|l| l.joint.as_ref().is_none_or(|j| joints.contains(j)));
        if applies {
            let items = shared.into_iter().map(|locator| Inherited {
                locator,
                source: Source::Preset(preset.from.clone()),
            });
            lay(&mut out, items, |i| &i.locator.name);
        }
    }
    Ok(out)
}

/// The locators of the preset model `from`: its file's and its sidecar's.
fn shared(input: &Path, from: &str) -> Result<Vec<Locator>, String> {
    let path = input.join(from);
    if !model::is_model(&path) {
        return Err(format!("{from} is not a glTF model"));
    }
    let data = std::fs::read(&path).map_err(|e| format!("{from}: {e}"))?;
    let mut out = model::file_locators(&data).map_err(|e| format!("{from}: {e}"))?;
    let sidecar = amod::read(&path)?.map(|a| a.locators).unwrap_or_default();
    lay(&mut out, sidecar, |l| &l.name);
    Ok(out)
}

/// What the model has: `inherited` with `sidecar` laid on it.
pub fn merged(inherited: &[Inherited], sidecar: &[Locator]) -> Vec<Locator> {
    let mut out: Vec<Locator> = inherited.iter().map(|i| i.locator.clone()).collect();
    lay(&mut out, sidecar.iter().cloned(), |l| &l.name);
    out
}

/// Every locator of the input model `rel`: its file's, its presets' and its sidecar's; see
/// docs/features/locators.md#attachment-preview.
pub fn model_locators(
    presets: &config::Locators,
    input: &Path,
    rel: &str,
) -> Result<Vec<Locator>, String> {
    let path = input.join(rel);
    let data = std::fs::read(&path).map_err(|e| format!("{rel}: {e}"))?;
    let inherited = inherited(presets, input, rel, &data)?;
    let sidecar = amod::read(&path)?.map(|a| a.locators).unwrap_or_default();
    Ok(merged(&inherited, &sidecar))
}

/// What the sidecar must hold for the model to have `locators`: those not inherited, or that
/// differ from what they replace.
pub fn sidecar_part(locators: &[Locator], inherited: &[Inherited]) -> Vec<Locator> {
    locators
        .iter()
        .filter(|l| {
            inherited
                .iter()
                .find(|i| i.locator.name == l.name)
                .is_none_or(|i| i.locator != **l)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(name: &str, x: f32) -> Locator {
        Locator {
            translation: [x, 0.0, 0.0],
            ..Locator::new(name.into())
        }
    }

    #[test]
    fn later_sources_replace_in_place() {
        let inherited = vec![
            Inherited {
                locator: at("a", 1.0),
                source: Source::File,
            },
            Inherited {
                locator: at("b", 1.0),
                source: Source::Preset("chars/hero.glb".into()),
            },
        ];
        let sidecar = [at("b", 2.0), at("c", 3.0), at("a", 1.0)];
        let merged = merged(&inherited, &sidecar);
        let names: Vec<_> = merged.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(merged[1].translation[0], 2.0);
        // Only what differs is kept: `a` equals the file's.
        let part = sidecar_part(&merged, &inherited);
        let names: Vec<_> = part.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["b", "c"]);
        assert_eq!(inherited[1].source.label(), "hero.glb");
    }

    /// A skinned model with joints `Hips` and `Hand`, `SOCKET_grip` under `Hand`, a node with
    /// `extras.locator` at the root and one named by its extras.
    fn rig(hand: &str) -> String {
        format!(
            r#"{{
            "asset": {{"version": "2.0"}},
            "nodes": [
                {{"name": "Hips", "children": [1]}},
                {{"name": "{hand}", "translation": [0, 1, 0], "children": [2]}},
                {{"name": "Offset", "translation": [0, 0, 1], "children": [3]}},
                {{"name": "SOCKET_grip", "translation": [1, 0, 0]}},
                {{"name": "Eye", "translation": [0, 2, 0], "extras": {{"locator": true}}}},
                {{"name": "Prop", "extras": {{"locator": "muzzle", "other": 1}}}},
                {{"name": "Plain", "extras": {{"locator": false}}}}
            ],
            "skins": [{{"joints": [0, 1]}}],
            "scenes": [{{"nodes": [0, 4, 5, 6]}}]
        }}"#
        )
    }

    #[test]
    fn files_carry_sockets_and_extras() {
        let locators = model::file_locators(rig("Hand").as_bytes()).unwrap();
        let names: Vec<_> = locators.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["grip", "Eye", "muzzle"]);
        // Under the joint, through the plain node between them.
        assert_eq!(locators[0].joint.as_deref(), Some("Hand"));
        assert_eq!(locators[0].translation, [1.0, 0.0, 1.0]);
        assert_eq!(locators[1].joint, None);
        assert_eq!(locators[1].translation, [0.0, 2.0, 0.0]);
    }

    #[test]
    fn presets_need_their_joints() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("hero.gltf"), rig("Hand")).unwrap();
        std::fs::write(
            dir.path().join("hero.gltf.amod"),
            "[[locators]]\nname = \"back\"\njoint = \"Hips\"\n",
        )
        .unwrap();
        let presets = config::Locators {
            presets: vec![config::LocatorPreset {
                from: "hero.gltf".into(),
                patterns: None,
            }],
        };
        // Same skeleton: the hero's locators (file and sidecar) after the model's own.
        let same = rig("Hand");
        let got = inherited(&presets, dir.path(), "villain.gltf", same.as_bytes()).unwrap();
        let names: Vec<_> = got.iter().map(|i| i.locator.name.as_str()).collect();
        assert_eq!(names, ["grip", "Eye", "muzzle", "back"]);
        assert_eq!(got[3].source, Source::Preset("hero.gltf".into()));
        assert_eq!(got[0].source, Source::Preset("hero.gltf".into()));
        // No `Hand` joint: not applied. The preset model itself: not applied.
        let other = rig("Paw");
        let got = inherited(&presets, dir.path(), "dog.gltf", other.as_bytes()).unwrap();
        assert!(got.iter().all(|i| i.source == Source::File));
        let got = inherited(&presets, dir.path(), "hero.gltf", same.as_bytes()).unwrap();
        assert_eq!(got.len(), 3);
        // Not matched by its globs.
        let globbed = config::Locators {
            presets: vec![config::LocatorPreset {
                from: "hero.gltf".into(),
                patterns: Some(vec!["chars/**".into()]),
            }],
        };
        let got = inherited(&globbed, dir.path(), "villain.gltf", same.as_bytes()).unwrap();
        assert_eq!(got.len(), 3);
        // A missing model fails.
        let missing = config::Locators {
            presets: vec![config::LocatorPreset {
                from: "nope.glb".into(),
                patterns: None,
            }],
        };
        let err = inherited(&missing, dir.path(), "villain.gltf", same.as_bytes()).unwrap_err();
        assert!(
            err.starts_with("assetc.toml [[locators.presets]] 1: nope.glb"),
            "{err}"
        );
    }

    #[test]
    fn a_models_locators_include_its_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("sword.gltf"), rig("Hand")).unwrap();
        std::fs::write(
            dir.path().join("sword.gltf.amod"),
            "[[locators]]\nname = \"grip\"\ntranslation = [0.0, 6.0, 0.0]\n\n[[locators]]\nname = \"tip\"\n",
        )
        .unwrap();
        let got = model_locators(&config::Locators::default(), dir.path(), "sword.gltf").unwrap();
        let names: Vec<_> = got.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["grip", "Eye", "muzzle", "tip"]);
        // The sidecar's grip replaces the file's, in model space.
        assert_eq!(got[0].joint, None);
        assert_eq!(got[0].translation, [0.0, 6.0, 0.0]);
        let err = model_locators(&config::Locators::default(), dir.path(), "nope.glb").unwrap_err();
        assert!(err.starts_with("nope.glb: "), "{err}");
    }
}
