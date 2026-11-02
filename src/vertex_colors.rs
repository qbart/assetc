//! Color sets: authored ones computed from vertex positions, blends and the order sets are
//! written in; see docs/features/vertex-colors.md.

use bevy::math::Vec3;

use crate::enrich::{BlendMode, ColorExport, ColorFill, ColorSet};
use crate::gradient;

/// An sRGB value 0–1 as a linear one.
fn decode(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A linear value 0–1 as an sRGB one.
fn encode(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// sRGB RGBA as stored in a set: R, G and B linear, alpha as it is.
pub fn stored([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    let linear = |c: u8| byte(decode(f32::from(c) / 255.0));
    [linear(r), linear(g), linear(b), a]
}

/// Position along the gradient of a vertex at `p`, 0 to 1.
fn position(set: &ColorSet, p: Vec3) -> f64 {
    let from = Vec3::from(set.from);
    let span = Vec3::from(set.to) - from;
    let length2 = span.length_squared();
    if length2 <= f32::EPSILON * f32::EPSILON {
        return 0.0;
    }
    let t = match set.fill {
        ColorFill::Solid => 0.0,
        ColorFill::Linear => (p - from).dot(span) / length2,
        ColorFill::Radial => (p - from).length() / length2.sqrt(),
    };
    f64::from(t.clamp(0.0, 1.0))
}

/// A set exported as Texture + UV buffer's coordinates of vertices at `positions`: `t`
/// along the gradient, and the texture's middle row; see
/// docs/features/vertex-colors.md#export.
pub fn uvs(set: &ColorSet, positions: &[Vec3]) -> Vec<[f32; 2]> {
    positions
        .iter()
        .map(|p| [position(set, *p) as f32, 0.5])
        .collect()
}

/// The texture of a set exported as Texture + UV buffer: its gradient, 256 × 1, sRGB.
pub fn texture(set: &ColorSet) -> image::RgbaImage {
    const WIDTH: u32 = 256;
    image::RgbaImage::from_fn(WIDTH, 1, |x, _| {
        let at = (f64::from(x) + 0.5) / f64::from(WIDTH);
        image::Rgba(gradient::color_at(&set.stops, at))
    })
}

/// Whether a set is written as a texture and UV set (Texture + UV buffer, or Both).
pub fn textured(set: &ColorSet) -> bool {
    set.export != ColorExport::Colors
}

/// Whether a set is written as colors (Color buffer, or Both).
pub fn colored(set: &ColorSet) -> bool {
    set.export != ColorExport::Texture
}

/// The sRGB color an authored set gives a vertex at `p`.
fn srgb(set: &ColorSet, p: Vec3) -> [u8; 4] {
    match set.fill {
        ColorFill::Solid => set.color,
        ColorFill::Linear | ColorFill::Radial => gradient::color_at(&set.stops, position(set, p)),
    }
}

/// The stored (linear) color an authored set gives a vertex at `p`, without blending.
pub fn evaluate(set: &ColorSet, p: Vec3) -> [u8; 4] {
    stored(srgb(set, p))
}

/// The model's whole list of sets: its glTF sets (`n` of `COLOR_n`, ascending) the sidecar
/// does not list, named `COLOR_n`, then the sidecar's; see docs/features/vertex-colors.md#order.
pub fn full_list(listed: &[ColorSet], model: &[u32]) -> Vec<ColorSet> {
    model
        .iter()
        .filter(|n| !listed.iter().any(|s| s.gltf == Some(**n)))
        .map(|&n| ColorSet::gltf(n, format!("COLOR_{n}")))
        .chain(listed.iter().cloned())
        .collect()
}

/// What the sidecar keeps of a whole list: all of it but a leading run of the model's glTF
/// sets, unrenamed and in ascending order, which [`full_list`] puts back.
pub fn sidecar_part(full: &[ColorSet], model: &[u32]) -> Vec<ColorSet> {
    let mut last = None;
    let lead = full
        .iter()
        .take_while(|s| match s.gltf {
            Some(n)
                if model.contains(&n)
                    && s.name == format!("COLOR_{n}")
                    && last.is_none_or(|l| l < n) =>
            {
                last = Some(n);
                true
            }
            _ => false,
        })
        .count();
    full[lead..].to_vec()
}

/// The index of the set `i` blends with, when it does and that set exists.
fn base(sets: &[ColorSet], i: usize) -> Option<usize> {
    let with = &sets[i].blend.as_ref()?.with;
    sets.iter().position(|s| s.name == *with)
}

/// Whether set `i` blends, directly or through others, with set `j`.
pub fn blends_with(sets: &[ColorSet], i: usize, j: usize) -> bool {
    let mut at = i;
    for _ in 0..sets.len() {
        match base(sets, at) {
            Some(b) if b == j => return true,
            Some(b) => at = b,
            None => return false,
        }
    }
    false
}

/// Names unique, blends with sets that exist and no loops; see
/// docs/features/vertex-colors.md#build.
pub fn check(sets: &[ColorSet]) -> Result<(), String> {
    for (i, set) in sets.iter().enumerate() {
        if sets[..i].iter().any(|s| s.name == set.name) {
            return Err(format!("color set {:?}: name used twice", set.name));
        }
    }
    for set in sets {
        let gradient = set.gltf.is_none() && set.fill != ColorFill::Solid;
        if textured(set) && (!gradient || set.blend.is_some()) {
            return Err(format!(
                "color set {:?}: Texture + UV buffer needs an unblended Linear or Radial set",
                set.name
            ));
        }
    }
    for (i, set) in sets.iter().enumerate() {
        if let Some(blend) = &set.blend {
            if base(sets, i).is_none() {
                return Err(format!(
                    "color set {:?}: blends with no set {:?}",
                    set.name, blend.with
                ));
            }
            if blends_with(sets, i, i) {
                return Err(format!("color set {:?}: blends in a loop", set.name));
            }
        }
    }
    Ok(())
}

/// Each set's name, when another set blends with it (so it is not written on its own).
pub fn bases(sets: &[ColorSet]) -> Vec<Option<&str>> {
    (0..sets.len())
        .map(|j| {
            (0..sets.len())
                .find(|&i| base(sets, i) == Some(j))
                .map(|i| sets[i].name.as_str())
        })
        .collect()
}

/// One primitive's glTF sets: `n` and a linear color per vertex.
pub type GltfSets = [(u32, Vec<[u8; 4]>)];

/// The glTF set `n`'s stored colors of a primitive with `count` vertices; white without it.
fn gltf_colors(gltf: &GltfSets, n: u32, count: usize) -> Vec<[u8; 4]> {
    match gltf.iter().find(|(m, _)| *m == n) {
        Some((_, colors)) => colors.clone(),
        None => vec![[255; 4]; count],
    }
}

/// Set `i`'s sRGB colors (0–1) at `positions`, blended; `depth` stops loops.
fn blended(
    sets: &[ColorSet],
    i: usize,
    positions: &[Vec3],
    gltf: &GltfSets,
    depth: usize,
) -> Vec<[f32; 4]> {
    let set = &sets[i];
    let own: Vec<[f32; 4]> = match set.gltf {
        Some(n) => gltf_colors(gltf, n, positions.len())
            .into_iter()
            .map(|[r, g, b, a]| {
                let srgb = |c: u8| encode(f32::from(c) / 255.0);
                [srgb(r), srgb(g), srgb(b), f32::from(a) / 255.0]
            })
            .collect(),
        None => positions
            .iter()
            .map(|p| srgb(set, *p).map(|c| f32::from(c) / 255.0))
            .collect(),
    };
    let (Some(blend), Some(b)) = (&set.blend, base(sets, i)) else {
        return own;
    };
    if depth >= sets.len() {
        return own;
    }
    let under = blended(sets, b, positions, gltf, depth + 1);
    under
        .into_iter()
        .zip(own)
        .map(|(b, t)| {
            let k = t[3] * blend.amount;
            let mut out = [0.0; 4];
            for c in 0..3 {
                let f = match blend.mode {
                    BlendMode::Mix => t[c],
                    BlendMode::Multiply => b[c] * t[c],
                    BlendMode::Screen => 1.0 - (1.0 - b[c]) * (1.0 - t[c]),
                    BlendMode::Overlay if b[c] < 0.5 => 2.0 * b[c] * t[c],
                    BlendMode::Overlay => 1.0 - 2.0 * (1.0 - b[c]) * (1.0 - t[c]),
                    BlendMode::Add => (b[c] + t[c]).min(1.0),
                };
                out[c] = b[c] + (f - b[c]) * k;
            }
            out[3] = b[3] + (1.0 - b[3]) * k;
            out
        })
        .collect()
}

/// Set `i`'s stored (linear RGBA8) colors of a primitive whose vertices are at `positions`
/// and whose glTF sets are `gltf`; see docs/features/vertex-colors.md#blending.
pub fn colors(sets: &[ColorSet], i: usize, positions: &[Vec3], gltf: &GltfSets) -> Vec<[u8; 4]> {
    let set = &sets[i];
    match (set.gltf, &set.blend) {
        (Some(n), _) => gltf_colors(gltf, n, positions.len()),
        (None, None) => positions.iter().map(|p| evaluate(set, *p)).collect(),
        (None, Some(_)) => blended(sets, i, positions, gltf, 0)
            .into_iter()
            .map(|[r, g, b, a]| [byte(decode(r)), byte(decode(g)), byte(decode(b)), byte(a)])
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradients_follow_the_line() {
        let mut set = ColorSet::new("g".into(), [0.0, 0.0, 0.0], [0.0, 2.0, 0.0]);
        // Below the start, at the end and past it.
        assert_eq!(evaluate(&set, Vec3::new(5.0, -1.0, 0.0)), [0, 0, 0, 255]);
        assert_eq!(evaluate(&set, Vec3::new(0.0, 2.0, 3.0)), [255; 4]);
        assert_eq!(evaluate(&set, Vec3::new(0.0, 9.0, 0.0)), [255; 4]);
        // Halfway: sRGB 128 (rounded from 127.5), stored linear.
        let middle = evaluate(&set, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(middle, [55, 55, 55, 255]);
        set.fill = ColorFill::Radial;
        assert_eq!(evaluate(&set, Vec3::new(2.0, 0.0, 0.0)), [255; 4]);
        assert_eq!(evaluate(&set, Vec3::new(0.0, -1.0, 0.0)), middle);
        // Ends at one point: the first stop.
        set.to = set.from;
        assert_eq!(evaluate(&set, Vec3::ONE), [0, 0, 0, 255]);
        set.fill = ColorFill::Solid;
        set.color = [255, 128, 0, 7];
        assert_eq!(evaluate(&set, Vec3::ONE), [255, 55, 0, 7]);
    }

    #[test]
    fn lists_keep_their_order() {
        let a = ColorSet::new("a".into(), [0.0; 3], [1.0; 3]);
        // COLOR_1 renamed after `a`: COLOR_0 and COLOR_2 come first.
        let listed = [a.clone(), ColorSet::gltf(1, "wear".into())];
        let full = full_list(&listed, &[0, 1, 2]);
        let names: Vec<&str> = full.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["COLOR_0", "COLOR_2", "a", "wear"]);
        assert_eq!(sidecar_part(&full, &[0, 1, 2]), listed);
        // `a` dragged first: every glTF set is then listed.
        let moved = [
            full[2].clone(),
            full[0].clone(),
            full[1].clone(),
            full[3].clone(),
        ];
        assert_eq!(sidecar_part(&moved, &[0, 1, 2]).len(), 4);
        assert_eq!(
            full_list(&sidecar_part(&moved, &[0, 1, 2]), &[0, 1, 2]),
            moved
        );
        // Unrenamed and in order: nothing to keep.
        let plain = full_list(&[], &[0, 1]);
        assert!(sidecar_part(&plain, &[0, 1]).is_empty());
    }

    #[test]
    fn blends_chain_and_loops_fail() {
        use crate::enrich::Blend;
        let blend = |with: &str| {
            Some(Blend {
                with: with.into(),
                mode: BlendMode::Mix,
                amount: 0.5,
            })
        };
        let mut sets = vec![
            ColorSet::new("a".into(), [0.0; 3], [1.0; 3]),
            ColorSet::new("b".into(), [0.0; 3], [1.0; 3]),
            ColorSet::new("c".into(), [0.0; 3], [1.0; 3]),
        ];
        sets[0].fill = ColorFill::Solid;
        sets[0].color = [255, 0, 0, 255];
        sets[1].fill = ColorFill::Solid;
        sets[1].color = [0, 0, 255, 255];
        sets[1].blend = blend("a");
        assert!(check(&sets).is_ok());
        assert_eq!(bases(&sets), [Some("b"), None, None]);
        assert!(blends_with(&sets, 1, 0) && !blends_with(&sets, 0, 1));
        // Half red, half blue in sRGB, stored linear.
        let half = byte(decode(0.5));
        assert_eq!(colors(&sets, 1, &[Vec3::ZERO], &[]), [[half, 0, half, 255]]);
        sets[2].blend = blend("b");
        assert!(blends_with(&sets, 2, 0));
        sets[0].blend = blend("c");
        assert!(check(&sets).unwrap_err().contains("blends in a loop"));
        sets[0].blend = blend("d");
        assert!(check(&sets).unwrap_err().contains("blends with no set"));
    }

    #[test]
    fn texture_exports_sample_their_gradient() {
        let mut set = ColorSet::new("g".into(), [0.0; 3], [0.0, 2.0, 0.0]);
        set.export = ColorExport::Texture;
        assert!(check(std::slice::from_ref(&set)).is_ok());
        let uv = uvs(&set, &[Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 3.0, 0.0)]);
        assert_eq!(uv, [[0.5, 0.5], [1.0, 0.5]]);
        let texture = texture(&set);
        assert_eq!(texture.dimensions(), (256, 1));
        assert_eq!(texture.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(texture.get_pixel(255, 0).0, [255; 4]);
        assert!(textured(&set) && !colored(&set));
        set.export = ColorExport::Both;
        assert!(textured(&set) && colored(&set) && check(std::slice::from_ref(&set)).is_ok());
        set.fill = ColorFill::Solid;
        assert!(check(&[set]).unwrap_err().contains("Texture + UV buffer"));
    }
}
