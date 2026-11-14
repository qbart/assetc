//! Custom mip chains: a texture's `.amipmap` file and the levels it gives; see
//! docs/features/mipmaps.md and docs/spec/amipmap.md.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::build::{is_texture_source, strip_extension};
use crate::plan::{resolve, split_dir};
use crate::texture::{self, ColorSpace, Image};

/// What a texture's file name gets to name its `.amipmap`.
pub const SUFFIX: &str = ".amipmap";

/// A parsed `.amipmap`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Amipmap {
    /// Levels written, level 0 included; `None`: the full chain.
    pub levels: Option<u32>,
    /// The chosen file of a level (1 and up), relative to the texture's folder.
    pub files: BTreeMap<u32, String>,
}

impl Amipmap {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_owned())?;
        let mut out = Self::default();
        if let Some(v) = table.remove("levels") {
            let n = v.as_integer().ok_or("levels must be an integer")?;
            let n = u32::try_from(n).ok().filter(|n| *n >= 1);
            out.levels = Some(n.ok_or("levels must be 1 or more")?);
        }
        if let Some(v) = table.remove("files") {
            let files = v.as_table().ok_or("[files] must be a table")?;
            for (key, value) in files {
                let n = level_number(key)
                    .ok_or_else(|| format!("[files] {key} is not a level (1 or more)"))?;
                let path = value
                    .as_str()
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| format!("[files] {key} must be a path"))?;
                if let Some(count) = out.levels.filter(|count| n >= *count) {
                    return Err(format!(
                        "[files] {key}: there is no level {n} with levels = {count}"
                    ));
                }
                out.files.insert(n, path.to_owned());
            }
        }
        if let Some(key) = table.keys().next() {
            return Err(format!("unknown key {key}"));
        }
        Ok(out)
    }

    /// The written form; see docs/spec/amipmap.md#written-form.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        if let Some(levels) = self.levels {
            out += &format!("levels = {levels}\n");
        }
        if !self.files.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out += "[files]\n";
            for (level, path) in &self.files {
                out += &format!("{level} = {}\n", toml::Value::String(path.clone()));
            }
        }
        out
    }
}

/// A level number as `[files]` and file names write it: decimal, 1 or more, no leading zeros.
fn level_number(text: &str) -> Option<u32> {
    let plain =
        !text.is_empty() && !text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit());
    text.parse().ok().filter(|_| plain)
}

/// The `.amipmap` of `texture`: its full file name plus the suffix.
pub fn path_for(texture: &Path) -> PathBuf {
    let mut name = texture.file_name().unwrap_or_default().to_os_string();
    name.push(SUFFIX);
    texture.with_file_name(name)
}

/// Whether `path` is an `.amipmap` file.
pub fn is_amipmap(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "amipmap")
}

/// Whether `file` is a texture that can have an `.amipmap`: an image or a noise.
pub fn applies_to(file: &Path) -> bool {
    crate::build::is_image(file) || crate::build::is_noise(file)
}

/// The `.amipmap` of `texture`, if it has one; errors name the file.
pub fn read(texture: &Path) -> Result<Option<Amipmap>, String> {
    let path = path_for(texture);
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match fs::read_to_string(&path) {
        Ok(text) => Amipmap::parse(&text)
            .map(Some)
            .map_err(|e| format!("{name}: {e}")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// Writes the `.amipmap` of `texture`.
pub fn write(texture: &Path, amipmap: &Amipmap) -> io::Result<()> {
    fs::write(path_for(texture), amipmap.to_toml())
}

/// The level that `file_name` is by name for the texture named `texture_name`:
/// `<name>.mip<n>.<ext>`, `<name>` the texture's name without its extension.
pub fn named_level(file_name: &str, texture_name: &str) -> Option<u32> {
    if !is_texture_source(Path::new(file_name)) {
        return None;
    }
    strip_extension(file_name)
        .strip_prefix(strip_extension(texture_name))?
        .strip_prefix(".mip")
        .and_then(level_number)
}

/// The files next to `texture` that are its levels by name, sorted.
pub fn named_files(texture: &Path) -> Vec<(u32, PathBuf)> {
    let name = texture.file_name().unwrap_or_default().to_string_lossy();
    let Some(entries) = texture.parent().and_then(|dir| fs::read_dir(dir).ok()) else {
        return Vec::new();
    };
    let mut out: Vec<(u32, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| {
            let level = named_level(&p.file_name()?.to_string_lossy(), &name)?;
            Some((level, p))
        })
        .collect();
    out.sort();
    out
}

/// A texture's custom chain as the build plans it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    /// Levels written, level 0 included; `None`: the full chain.
    pub levels: Option<u32>,
    /// The levels that come from files.
    pub files: BTreeMap<u32, LevelFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelFile {
    /// Input-relative.
    pub path: String,
    /// What names it to the user: the path as `[files]` has it, or the name of a file by name.
    pub shown: String,
    /// Found by its name rather than chosen in `[files]`.
    pub named: bool,
}

/// What [`chains`] finds.
#[derive(Default)]
pub struct Chains {
    /// The custom chain of each texture that has one, by its input-relative path.
    pub chains: HashMap<String, Chain>,
    /// Every file that is a level of a texture (input-relative): not built on its own.
    pub level_files: HashSet<String>,
    /// Problems, as (the `.amipmap`, message).
    pub failures: Vec<(String, String)>,
}

/// Reads every `.amipmap` among `files` (the input's files, input-relative); see
/// docs/features/mipmaps.md#build.
pub fn chains(input: &Path, files: &[String]) -> Chains {
    let listing: HashSet<&str> = files.iter().map(String::as_str).collect();
    let mut out = Chains::default();
    // Each level file and a texture it is a level of.
    let mut level_of: HashMap<String, String> = HashMap::new();
    for rel in files {
        let Some(texture) = rel.strip_suffix(SUFFIX) else {
            continue;
        };
        let (dir, name) = split_dir(texture);
        let mut fail = |message: String| out.failures.push((rel.clone(), message));
        if !listing.contains(texture) || !is_texture_source(Path::new(texture)) {
            fail(format!("no texture {name} next to it"));
            continue;
        }
        if !applies_to(Path::new(texture)) {
            fail("gradients and palettes have no mipmaps".into());
            continue;
        }
        // Files by name are the texture's, whether or not they end up used.
        let mut named: BTreeMap<u32, Vec<&String>> = BTreeMap::new();
        for file in files {
            let (file_dir, file_name) = split_dir(file);
            if file_dir == dir
                && let Some(level) = named_level(file_name, name)
            {
                named.entry(level).or_default().push(file);
                level_of.insert(file.clone(), texture.to_owned());
            }
        }
        let amipmap = match fs::read_to_string(input.join(rel))
            .map_err(|e| e.to_string())
            .and_then(|text| Amipmap::parse(&text))
        {
            Ok(amipmap) => amipmap,
            Err(message) => {
                fail(message);
                continue;
            }
        };
        let mut chain = Chain {
            levels: amipmap.levels,
            files: BTreeMap::new(),
        };
        let mut ok = true;
        for (level, found) in named {
            if amipmap.files.contains_key(&level) || amipmap.levels.is_some_and(|n| level >= n) {
                continue;
            }
            if let [a, b, ..] = found.as_slice() {
                let (a, b) = (split_dir(a).1, split_dir(b).1);
                fail(format!("level {level} has two files by name: {a} and {b}"));
                ok = false;
                continue;
            }
            chain.files.insert(
                level,
                LevelFile {
                    path: found[0].clone(),
                    shown: split_dir(found[0]).1.to_owned(),
                    named: true,
                },
            );
        }
        for (level, path) in &amipmap.files {
            let found = resolve(dir, path).filter(|r| {
                listing.contains(r.as_str()) && is_texture_source(Path::new(r)) && r != texture
            });
            let Some(found) = found else {
                fail(format!(
                    "level {level} = {path:?} is not a texture source in input"
                ));
                ok = false;
                continue;
            };
            level_of.insert(found.clone(), texture.to_owned());
            chain.files.insert(
                *level,
                LevelFile {
                    path: found,
                    shown: path.clone(),
                    named: false,
                },
            );
        }
        if ok {
            out.chains.insert(texture.to_owned(), chain);
        }
    }
    // A level is not a texture, so it has no chain of its own.
    let mut nested: Vec<(String, String)> = out
        .chains
        .keys()
        .filter_map(|t| Some((t.clone(), level_of.get(t)?.clone())))
        .collect();
    nested.sort();
    for (texture, other) in nested {
        out.chains.remove(&texture);
        let message = format!(
            "{} is a mip level of {}",
            split_dir(&texture).1,
            split_dir(&other).1
        );
        out.failures.push((format!("{texture}{SUFFIX}"), message));
    }
    out.level_files = level_of.into_keys().collect();
    out
}

/// One level of a custom chain.
pub struct Level {
    pub image: Image,
    /// The file it comes from; `None` for level 0 and generated levels.
    pub file: Option<LevelFile>,
    /// Why its file could not be used (the level is then generated).
    pub problem: Option<String>,
}

/// The levels `chain` gives the texture `base`, level 0 first: each from its file (read by
/// `load`, by input-relative path) or generated from the level above. Problems are kept
/// with the levels generated instead, so there is always something to show; the chain's own
/// problem is returned too.
pub fn levels(
    base: Image,
    color: ColorSpace,
    chain: &Chain,
    mut load: impl FnMut(&str) -> Result<Image, String>,
) -> (Vec<Level>, Option<String>) {
    let (width, height) = (base.width, base.height);
    let full = texture::level_count(width, height);
    let mut problem = None;
    let count = match chain.levels {
        Some(n) if n as usize > full => {
            problem = Some(format!(
                "levels = {n}, but a {width} × {height} texture has at most {full}"
            ));
            full
        }
        Some(n) => n as usize,
        None => full,
    };
    // Files by name past the chain are just not used.
    if let Some((level, file)) = chain
        .files
        .iter()
        .find(|(level, file)| !file.named && **level as usize >= full)
    {
        problem.get_or_insert_with(|| {
            format!(
                "level {level} ({}): a {width} × {height} texture has only {full} levels",
                file.shown
            )
        });
    }
    let mut out = vec![Level {
        image: base,
        file: None,
        problem: None,
    }];
    for level in 1..count {
        let above = &out[level - 1].image;
        let size = ((above.width / 2).max(1), (above.height / 2).max(1));
        let file = chain.files.get(&(level as u32));
        let loaded = file.map(|file| {
            let image =
                load(&file.path).map_err(|e| format!("level {level} ({}): {e}", file.shown))?;
            if (image.width, image.height) != size {
                return Err(format!(
                    "level {level} ({}) is {} × {}, not {} × {}",
                    file.shown, image.width, image.height, size.0, size.1
                ));
            }
            Ok(image)
        });
        let next = match loaded {
            Some(Ok(image)) => Level {
                image,
                file: file.cloned(),
                problem: None,
            },
            Some(Err(problem)) => Level {
                image: texture::downsample(above, color),
                file: file.cloned(),
                problem: Some(problem),
            },
            None => Level {
                image: texture::downsample(above, color),
                file: None,
                problem: None,
            },
        };
        out.push(next);
    }
    (out, problem)
}

/// The levels to write for `base` and how many of them come from files; any problem fails.
pub fn build(
    base: Image,
    color: ColorSpace,
    chain: &Chain,
    load: impl FnMut(&str) -> Result<Image, String>,
) -> Result<(Vec<Image>, usize), String> {
    let (levels, problem) = levels(base, color, chain, load);
    if let Some(problem) = problem {
        return Err(problem);
    }
    if let Some(problem) = levels.iter().find_map(|l| l.problem.clone()) {
        return Err(problem);
    }
    let custom = levels.iter().filter(|l| l.file.is_some()).count();
    Ok((levels.into_iter().map(|l| l.image).collect(), custom))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: u32, height: u32, value: u8) -> Image {
        Image {
            width,
            height,
            pixels: vec![value; (width * height * 4) as usize],
        }
    }

    fn file(path: &str, named: bool) -> LevelFile {
        LevelFile {
            path: path.into(),
            shown: path.into(),
            named,
        }
    }

    #[test]
    fn parses_and_writes() {
        assert_eq!(Amipmap::parse("").unwrap(), Amipmap::default());
        let text = "levels = 6\n\n[files]\n1 = \"rock_half.png\"\n3 = \"../shared/gray 32.png\"\n";
        let parsed = Amipmap::parse(text).unwrap();
        assert_eq!(parsed.levels, Some(6));
        assert_eq!(parsed.files[&1], "rock_half.png");
        assert_eq!(parsed.files[&3], "../shared/gray 32.png");
        assert_eq!(parsed.to_toml(), text);
        assert_eq!(Amipmap::default().to_toml(), "");
        let only_files = Amipmap::parse("[files]\n10 = \"a.png\"\n2 = \"b.png\"\n").unwrap();
        assert_eq!(
            only_files.to_toml(),
            "[files]\n2 = \"b.png\"\n10 = \"a.png\"\n"
        );
    }

    #[test]
    fn rejects_what_the_format_does_not_allow() {
        for (text, problem) in [
            ("levels = 0", "levels must be 1 or more"),
            ("levels = \"3\"", "levels must be an integer"),
            ("level = 3", "unknown key level"),
            ("files = 3", "[files] must be a table"),
            (
                "[files]\n0 = \"a.png\"",
                "[files] 0 is not a level (1 or more)",
            ),
            (
                "[files]\n01 = \"a.png\"",
                "[files] 01 is not a level (1 or more)",
            ),
            (
                "[files]\nbase = \"a.png\"",
                "[files] base is not a level (1 or more)",
            ),
            ("[files]\n1 = \"\"", "[files] 1 must be a path"),
            ("[files]\n1 = 2", "[files] 1 must be a path"),
            (
                "levels = 2\n[files]\n2 = \"a.png\"",
                "[files] 2: there is no level 2 with levels = 2",
            ),
        ] {
            assert_eq!(Amipmap::parse(text).unwrap_err(), problem, "{text}");
        }
    }

    #[test]
    fn levels_by_name() {
        assert_eq!(named_level("rock.mip1.png", "rock.png"), Some(1));
        assert_eq!(named_level("rock.mip12.tga", "rock.png"), Some(12));
        assert_eq!(
            named_level("brick.normal.mip2.png", "brick.normal.png"),
            Some(2)
        );
        for other in [
            "rock.mip0.png",
            "rock.mip01.png",
            "rock.mip.png",
            "rock.mip1.txt",
            "rock.mip1",
            "rocks.mip1.png",
            "rock.png",
            "rock.MIP1.png",
        ] {
            assert_eq!(named_level(other, "rock.png"), None, "{other}");
        }
        assert_eq!(
            path_for(Path::new("a/rock.png")),
            Path::new("a/rock.png.amipmap")
        );
        assert!(is_amipmap(Path::new("a/rock.png.amipmap")));
        assert!(!is_amipmap(Path::new("a/rock.png")));
    }

    #[test]
    fn levels_come_from_files_or_the_level_above() {
        let chain = Chain {
            levels: None,
            files: BTreeMap::from([(1, file("one.png", true)), (3, file("three.png", false))]),
        };
        let load = |path: &str| match path {
            "one.png" => Ok(flat(4, 2, 200)),
            "three.png" => Ok(flat(1, 1, 7)),
            other => Err(format!("no {other}")),
        };
        let (levels, custom) = build(flat(8, 4, 10), ColorSpace::Linear, &chain, load).unwrap();
        let seen: Vec<_> = levels
            .iter()
            .map(|l| (l.width, l.height, l.pixels[0]))
            .collect();
        // Level 2 is generated from the custom level 1, not from the texture.
        assert_eq!(seen, [(8, 4, 10), (4, 2, 200), (2, 1, 200), (1, 1, 7)]);
        assert_eq!(custom, 2);
    }

    #[test]
    fn levels_limits_the_chain() {
        let chain = |levels| Chain {
            levels: Some(levels),
            files: BTreeMap::new(),
        };
        let none = |_: &str| Err("unused".to_owned());
        let (levels, custom) = build(flat(8, 8, 1), ColorSpace::Srgb, &chain(2), none).unwrap();
        assert_eq!((levels.len(), custom), (2, 0));
        let (levels, _) = build(flat(8, 8, 1), ColorSpace::Srgb, &chain(1), none).unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(
            build(flat(8, 8, 1), ColorSpace::Srgb, &chain(5), none).unwrap_err(),
            "levels = 5, but a 8 × 8 texture has at most 4"
        );
    }

    #[test]
    fn level_files_must_fit() {
        let chain = |level, named| Chain {
            levels: None,
            files: BTreeMap::from([(level, file("x.png", named))]),
        };
        let load = |_: &str| Ok(flat(3, 3, 0));
        assert_eq!(
            build(flat(8, 8, 1), ColorSpace::Linear, &chain(1, false), load).unwrap_err(),
            "level 1 (x.png) is 3 × 3, not 4 × 4"
        );
        let missing = |_: &str| Err("no such file".to_owned());
        assert_eq!(
            build(flat(8, 8, 1), ColorSpace::Linear, &chain(2, true), missing).unwrap_err(),
            "level 2 (x.png): no such file"
        );
        assert_eq!(
            build(flat(8, 8, 1), ColorSpace::Linear, &chain(4, false), load).unwrap_err(),
            "level 4 (x.png): a 8 × 8 texture has only 4 levels"
        );
        // A file by name past the chain is not used.
        let (levels, custom) =
            build(flat(8, 8, 1), ColorSpace::Linear, &chain(4, true), load).unwrap();
        assert_eq!((levels.len(), custom), (4, 0));
        // For the Inspector, a failed level is generated and keeps its problem.
        let (shown, problem) = levels_of(&chain(1, false));
        assert_eq!(problem, None);
        assert_eq!(
            shown[1].problem.as_deref(),
            Some("level 1 (x.png) is 3 × 3, not 4 × 4")
        );
        assert_eq!((shown[1].image.width, shown.len()), (4, 4));
    }

    fn levels_of(chain: &Chain) -> (Vec<Level>, Option<String>) {
        levels(flat(8, 8, 1), ColorSpace::Linear, chain, |_| {
            Ok(flat(3, 3, 0))
        })
    }

    #[test]
    fn finds_chains_in_the_input() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path();
        let write = |rel: &str, text: &str| {
            let path = input.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        };
        write(
            "t/rock.png.amipmap",
            "levels = 4\n[files]\n2 = \"../shared/gray.png\"\n",
        );
        write("t/plain.png.amipmap", "");
        write("t/bad.png.amipmap", "levels = 0\n");
        write("t/gone.png.amipmap", "");
        write("t/ramp.agradient.amipmap", "");
        write("t/two.png.amipmap", "");
        write("t/far.png.amipmap", "[files]\n1 = \"nowhere.png\"\n");
        write("t/rock.mip1.png.amipmap", "");
        let files: Vec<String> = [
            "shared/gray.png",
            "t/bad.png",
            "t/bad.png.amipmap",
            "t/far.png",
            "t/far.png.amipmap",
            "t/gone.png.amipmap",
            "t/plain.png",
            "t/plain.png.amipmap",
            "t/ramp.agradient",
            "t/ramp.agradient.amipmap",
            "t/rock.mip1.png",
            "t/rock.mip1.png.amipmap",
            "t/rock.mip2.png",
            "t/rock.mip7.png",
            "t/rock.png",
            "t/rock.png.amipmap",
            "t/two.mip1.jpg",
            "t/two.mip1.png",
            "t/two.png",
            "t/two.png.amipmap",
            "t/unrelated.mip1.png",
        ]
        .map(String::from)
        .to_vec();
        let found = chains(input, &files);

        let rock = &found.chains["t/rock.png"];
        assert_eq!(rock.levels, Some(4));
        // Level 1 by name; level 2 chosen, over the file by name; level 7 past `levels`.
        assert_eq!(
            rock.files,
            BTreeMap::from([
                (
                    1,
                    LevelFile {
                        path: "t/rock.mip1.png".into(),
                        shown: "rock.mip1.png".into(),
                        named: true
                    }
                ),
                (
                    2,
                    LevelFile {
                        path: "shared/gray.png".into(),
                        shown: "../shared/gray.png".into(),
                        named: false
                    }
                ),
            ])
        );
        assert_eq!(found.chains["t/plain.png"], Chain::default());
        let mut keys: Vec<_> = found.chains.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, ["t/plain.png", "t/rock.png"]);

        let mut level_files: Vec<_> = found.level_files.iter().cloned().collect();
        level_files.sort();
        assert_eq!(
            level_files,
            [
                "shared/gray.png",
                "t/rock.mip1.png",
                "t/rock.mip2.png",
                "t/rock.mip7.png",
                "t/two.mip1.jpg",
                "t/two.mip1.png"
            ]
        );

        let mut failures = found.failures.clone();
        failures.sort();
        let failures: Vec<_> = failures
            .iter()
            .map(|(file, message)| (file.as_str(), message.as_str()))
            .collect();
        assert_eq!(
            failures,
            [
                ("t/bad.png.amipmap", "levels must be 1 or more"),
                (
                    "t/far.png.amipmap",
                    "level 1 = \"nowhere.png\" is not a texture source in input"
                ),
                ("t/gone.png.amipmap", "no texture gone.png next to it"),
                (
                    "t/ramp.agradient.amipmap",
                    "gradients and palettes have no mipmaps"
                ),
                (
                    "t/rock.mip1.png.amipmap",
                    "rock.mip1.png is a mip level of rock.png"
                ),
                (
                    "t/two.png.amipmap",
                    "level 1 has two files by name: two.mip1.jpg and two.mip1.png"
                ),
            ]
        );
    }
}
