//! Making one asset from several selected files: so far a PBR material from textures; see
//! docs/features/groups.md.

use std::path::{Path, PathBuf};

use crate::material::{NormalY, Slot, Source};

const SEPARATORS: [char; 4] = ['_', '-', '.', ' '];

/// What a word in a file name says about its slot; see docs/features/groups.md#slots.
fn keyword(word: &str) -> Option<Option<Slot>> {
    Some(Some(match word {
        "basecolor" | "albedo" | "diffuse" | "diff" | "color" | "colour" | "col" | "alb" => {
            Slot::BaseColor
        }
        "normal" | "normals" | "nor" | "nrm" | "norm" | "n" => Slot::Normal,
        "roughness" | "rough" | "rgh" | "r" => Slot::Roughness,
        "metallic" | "metalness" | "metal" | "met" | "m" => Slot::Metallic,
        "ao" | "occlusion" | "occ" | "ambient" => Slot::Occlusion,
        "emissive" | "emission" | "emit" | "glow" | "e" => Slot::Emissive,
        "height" | "displacement" | "disp" | "bump" | "h" => Slot::Height,
        "arm" | "orm" | "rma" | "mra" | "gloss" | "glossiness" | "spec" | "specular"
        | "opacity" | "alpha" | "mask" | "cavity" | "preview" | "thumb" => return Some(None),
        _ => return None,
    }))
}

/// `2k`, `4K`, `1024`, …: texture resolutions in file names.
fn is_resolution(piece: &str) -> bool {
    let piece = piece.to_ascii_lowercase();
    matches!(piece.as_str(), "512" | "1024" | "2048" | "4096" | "8192")
        || piece
            .strip_suffix('k')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The lowercase words of `piece`, split at lower-to-upper case changes and between letters and
/// digits: `NormalGL` → `normal`, `gl`.
fn words(piece: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut prev: Option<char> = None;
    for c in piece.chars() {
        let split = prev.is_some_and(|p| {
            (p.is_lowercase() && c.is_uppercase()) || (p.is_ascii_digit() != c.is_ascii_digit())
        });
        if split || out.is_empty() {
            out.push(String::new());
        }
        out.last_mut().expect("a word").extend(c.to_lowercase());
        prev = Some(c);
    }
    out
}

/// The words of `text` that can name a slot: separator pieces that are not resolutions, split
/// into words, without digits-only words.
fn slot_words(text: &str) -> Vec<String> {
    text.split(SEPARATORS)
        .filter(|p| !p.is_empty() && !is_resolution(p))
        .flat_map(words)
        .filter(|w| !w.bytes().all(|b| b.is_ascii_digit()))
        .collect()
}

/// What the words of `text` say, read from the last word back: the slot (or not used) of the
/// first keyword.
fn decide(text: &str) -> Option<Option<Slot>> {
    slot_words(text).iter().rev().find_map(|w| keyword(w))
}

/// A file's slot as guessed from its name without extension, and whether it says DirectX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guess {
    pub slot: Option<Slot>,
    pub dx: bool,
}

/// The slot of the file named `stem` in material `name`; see docs/features/groups.md#slots.
pub fn guess_slot(stem: &str, name: &str) -> Guess {
    let rest = stem.strip_prefix(name).unwrap_or(stem);
    if rest.is_empty() {
        return Guess {
            slot: Some(Slot::BaseColor),
            dx: false,
        };
    }
    let words = slot_words(rest);
    Guess {
        slot: decide(rest).flatten(),
        dx: words.iter().any(|w| w == "dx" || w == "directx"),
    }
}

/// The material name the files named `stems` (without extensions) share; see
/// docs/features/groups.md#name.
pub fn guess_name(stems: &[&str]) -> String {
    let Some(first) = stems.first() else {
        return "material".into();
    };
    let mut len = stems.iter().fold(first.len(), |len, stem| {
        first
            .char_indices()
            .zip(stem.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8())
            .min(len)
    });
    // Cut back to a separator or the end of every name.
    let at_boundary = |len: usize| {
        stems
            .iter()
            .all(|s| s.len() == len || s[len..].starts_with(SEPARATORS))
    };
    while len > 0 && !at_boundary(len) {
        len = first[..len].char_indices().last().map_or(0, |(i, _)| i);
    }
    let whole = |name: &str| stems.iter().any(|s| *s == name);
    // Nothing shared: the first name.
    let mut name = if len == 0 { first } else { &first[..len] };
    loop {
        name = name.trim_end_matches(SEPARATORS);
        let Some((head, last)) = name.rsplit_once(SEPARATORS) else {
            break;
        };
        // Resolutions go; so does a slot word ending a whole file name.
        if is_resolution(last) || (whole(name) && decide(last).is_some()) {
            name = head;
        } else {
            break;
        }
    }
    let name = if name.is_empty() || is_resolution(name) {
        first.trim_end_matches(SEPARATORS)
    } else {
        name
    };
    match name.replace('.', "_") {
        name if name.is_empty() => "material".into(),
        name => name,
    }
}

/// The file name without its extension.
fn stem(path: &Path) -> &str {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    crate::build::strip_extension(name)
}

fn extension(path: &Path) -> &str {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
}

/// A material about to be made from selected files; see docs/features/groups.md#make-material.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPlan {
    pub name: String,
    /// Each file and its slot (`None`: not used), in tree order.
    pub files: Vec<(PathBuf, Option<Slot>)>,
    pub normal_y: NormalY,
    /// Put the files in a new folder `<name>`.
    pub folder: bool,
    /// Rename the files to the naming convention.
    pub rename: bool,
}

impl MaterialPlan {
    /// The plan with everything guessed from the files' names (and the folder option from what
    /// else their folder holds).
    pub fn new(files: Vec<PathBuf>) -> Self {
        let stems: Vec<&str> = files.iter().map(|f| stem(f)).collect();
        let name = guess_name(&stems);
        let guesses: Vec<Guess> = stems.iter().map(|s| guess_slot(s, &name)).collect();
        let mut slots: Vec<Option<Slot>> = guesses.iter().map(|g| g.slot).collect();
        // One file per slot: the first, but an OpenGL normal map over a DirectX one.
        for slot in Slot::ALL {
            let taking: Vec<usize> = (0..files.len())
                .filter(|&i| slots[i] == Some(slot))
                .collect();
            let keep = taking
                .iter()
                .copied()
                .find(|&i| slot != Slot::Normal || !guesses[i].dx)
                .or(taking.first().copied());
            for i in taking {
                if Some(i) != keep {
                    slots[i] = None;
                }
            }
        }
        let dx = (0..files.len()).any(|i| slots[i] == Some(Slot::Normal) && guesses[i].dx);
        let mut plan = Self {
            name,
            files: files.into_iter().zip(slots).collect(),
            normal_y: if dx { NormalY::Dx } else { NormalY::Gl },
            folder: false,
            rename: true,
        };
        plan.folder = holds_others(&plan.source_dir(), &plan.paths());
        plan
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|(f, _)| f.clone()).collect()
    }

    /// The used files and their slots.
    pub fn used(&self) -> impl Iterator<Item = (&Path, Slot)> {
        self.files
            .iter()
            .filter_map(|(f, slot)| slot.map(|s| (f.as_path(), s)))
    }

    /// The files' folder, or the first file's when they come from several.
    pub fn source_dir(&self) -> PathBuf {
        let dir = |f: &PathBuf| f.parent().unwrap_or(Path::new("")).to_path_buf();
        self.files.first().map(|(f, _)| dir(f)).unwrap_or_default()
    }

    /// Where the material's files go: the new folder, or the files' folder.
    pub fn target_dir(&self) -> PathBuf {
        let dir = self.source_dir();
        if self.folder {
            dir.join(&self.name)
        } else {
            dir
        }
    }

    /// The `.amat` file.
    pub fn amat(&self) -> PathBuf {
        self.target_dir()
            .join(format!("{}{}", self.name, crate::material::SOURCE_SUFFIX))
    }

    /// Where `file` in `slot` ends up.
    pub fn destination(&self, file: &Path, slot: Slot) -> PathBuf {
        if !self.folder && !self.rename {
            return file.to_path_buf();
        }
        let name = if self.rename {
            let ext = extension(file);
            match slot.suffix() {
                Some(suffix) => format!("{}.{suffix}.{ext}", self.name),
                None => format!("{}.{ext}", self.name),
            }
        } else {
            file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        self.target_dir().join(name)
    }

    /// Why the material cannot be made as it is; empty when it can.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.name.trim().is_empty() {
            out.push("Name the material".into());
        } else if self.name.contains(['/', '\\', '.']) || self.name != self.name.trim() {
            out.push("The name cannot contain / \\ or .".into());
        }
        for slot in Slot::ALL {
            if self.used().filter(|(_, s)| *s == slot).count() > 1 {
                out.push(format!("{} has more than one file", slot.label()));
            }
        }
        if self.used().next().is_none() {
            out.push("No file is used".into());
        }
        if !out.is_empty() {
            return out;
        }
        let mut taken: Vec<PathBuf> = vec![self.amat()];
        if self.amat().exists() {
            out.push(exists(&self.amat()));
        }
        for (file, slot) in self.used() {
            let to = self.destination(file, slot);
            let replaces = to.exists() && !same_file(file, &to);
            if replaces || taken.contains(&to) {
                out.push(exists(&to));
            }
            taken.push(to);
        }
        out.dedup();
        out
    }

    /// The `.amat`'s contents: the normal map's convention and, without renaming, each used
    /// file's path relative to it.
    pub fn source(&self) -> Source {
        let mut source = Source::default();
        if self.normal_y == NormalY::Dx && self.used().any(|(_, s)| s == Slot::Normal) {
            source.normal_y = Some(NormalY::Dx);
        }
        if !self.rename {
            let dir = self.target_dir();
            for (file, slot) in self.used() {
                let to = self.destination(file, slot);
                source
                    .textures
                    .set(slot, Some(crate::adata::relative(&dir, &to)));
            }
        }
        source
    }
}

fn exists(path: &Path) -> String {
    let name = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let folder = path.parent().map(name).unwrap_or_default();
    format!("{} already exists in {folder}", name(path))
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Whether `dir` holds entries other than `files` and their companions (hidden ones aside).
fn holds_others(dir: &Path, files: &[PathBuf]) -> bool {
    let ours: Vec<PathBuf> = files
        .iter()
        .flat_map(|f| crate::fileops::companions(None, f))
        .collect();
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            let path = e.path();
            !e.file_name().to_string_lossy().starts_with('.') && !ours.contains(&path)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_share_a_start() {
        let name = |stems: &[&str]| guess_name(stems);
        assert_eq!(
            name(&["Metal007_2K_Color", "Metal007_2K_NormalGL"]),
            "Metal007"
        );
        assert_eq!(name(&["brick", "brick.n"]), "brick");
        assert_eq!(
            name(&["rock_wall_diff_4k", "rock_wall_nor_gl_4k"]),
            "rock_wall"
        );
        // Cut at a separator, not inside a word.
        assert_eq!(name(&["stone_albedo", "stones_normal"]), "stone");
        assert_eq!(name(&["rock_diff_4k"]), "rock");
        assert_eq!(name(&["grass"]), "grass");
        assert_eq!(name(&["4k"]), "4k");
        assert_eq!(name(&["my.wall_n", "my.wall_r"]), "my_wall");
        assert_eq!(name(&[]), "material");
    }

    #[test]
    fn slots_come_from_the_last_slot_word() {
        let slot = |stem: &str, name: &str| guess_slot(stem, name).slot;
        assert_eq!(slot("Metal007_2K_Color", "Metal007"), Some(Slot::BaseColor));
        assert_eq!(
            slot("Metal007_2K_Metalness", "Metal007"),
            Some(Slot::Metallic)
        );
        assert_eq!(slot("Metal007_2K_NormalGL", "Metal007"), Some(Slot::Normal));
        assert_eq!(
            slot("Metal007_2K_AmbientOcclusion", "Metal007"),
            Some(Slot::Occlusion)
        );
        assert_eq!(
            slot("Metal007_2K_Displacement", "Metal007"),
            Some(Slot::Height)
        );
        assert_eq!(slot("rock_wall_nor_gl_4k", "rock_wall"), Some(Slot::Normal));
        assert_eq!(slot("rock_wall_arm_4k", "rock_wall"), None);
        assert_eq!(slot("brick.ao", "brick"), Some(Slot::Occlusion));
        assert_eq!(slot("brick", "brick"), Some(Slot::BaseColor));
        assert_eq!(slot("brick_preview", "brick"), None);
        assert_eq!(slot("brick_extra", "brick"), None);
        assert!(guess_slot("wall_NormalDX", "wall").dx);
        assert!(!guess_slot("wall_NormalGL", "wall").dx);
    }

    fn files(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
        names
            .iter()
            .map(|n| {
                let path = dir.join(n);
                std::fs::write(&path, b"").unwrap();
                path
            })
            .collect()
    }

    #[test]
    fn plans_take_one_file_per_slot() {
        let dir = tempfile::tempdir().unwrap();
        let plan = MaterialPlan::new(files(
            dir.path(),
            &[
                "Metal_Color.png",
                "Metal_NormalDX.png",
                "Metal_NormalGL.png",
                "Metal_Roughness.png",
            ],
        ));
        assert_eq!(plan.name, "Metal");
        let slots: Vec<_> = plan.files.iter().map(|(_, s)| *s).collect();
        assert_eq!(
            slots,
            [
                Some(Slot::BaseColor),
                None,
                Some(Slot::Normal),
                Some(Slot::Roughness)
            ]
        );
        assert_eq!(plan.normal_y, NormalY::Gl);
        // Nothing else in the folder: no new folder.
        assert!(!plan.folder && plan.rename);
        assert_eq!(
            plan.destination(&plan.files[2].0, Slot::Normal),
            dir.path().join("Metal.n.png")
        );
        assert!(plan.problems().is_empty(), "{:?}", plan.problems());
        assert_eq!(plan.source().to_toml(), "");
    }

    #[test]
    fn problems_and_paths_without_renaming() {
        let dir = tempfile::tempdir().unwrap();
        let mut plan = MaterialPlan::new(files(
            dir.path(),
            &["wall_albedo.jpg", "wall_nrm_dx.jpg", "other.png"],
        ));
        // `other.png` is no part of the files: they get their own folder.
        plan.files.pop();
        plan.folder = true;
        assert_eq!(plan.normal_y, NormalY::Dx);
        plan.rename = false;
        assert_eq!(
            plan.source().to_toml(),
            "normal_y = \"dx\"\n\n[textures]\nbase_color = \"wall_albedo.jpg\"\nnormal = \"wall_nrm_dx.jpg\"\n"
        );
        plan.folder = false;
        plan.files[1].1 = Some(Slot::BaseColor);
        assert_eq!(plan.problems(), ["Base color has more than one file"]);
        plan.files[1].1 = Some(Slot::Normal);
        std::fs::write(dir.path().join("wall.amat"), "").unwrap();
        assert_eq!(
            plan.problems(),
            ["wall.amat already exists in ".to_owned()
                + &dir.path().file_name().unwrap().to_string_lossy()]
        );
        plan.name = "a.b".into();
        assert_eq!(plan.problems(), ["The name cannot contain / \\ or ."]);
    }
}
