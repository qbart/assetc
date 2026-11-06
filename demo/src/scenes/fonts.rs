//! Every font drawn with its SDF atlas, laid out by the SDK.

use assetc as sdk;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::{BAR, InScene, Spawn, TEXT, TEXT_WEAK};

const LINES: [&str; 2] = [
    "The quick brown fox jumps over the lazy dog.",
    "0123456789 !?&*()[]{} <=> +-/%",
];
const SIZES: [f32; 3] = [16.0, 32.0, 64.0];

pub fn spawn(s: &mut Spawn) {
    let Some(assets) = s.demo.assets.clone() else {
        return;
    };
    let (width, height) = (s.window.width(), s.window.height());
    let left = -width / 2.0 + 48.0;
    let mut baseline = height / 2.0 - BAR - 40.0;
    for entry in assets.of_kind(sdk::Kind::Font) {
        let Ok(font) = assets.load::<sdk::Font>(entry.id) else {
            continue;
        };
        let Ok(atlas) = assets.load::<sdk::Texture>(font.atlas) else {
            continue;
        };
        let Some(coverage) = coverage(&font, &atlas) else {
            continue;
        };
        let image = s.images.add(coverage);
        let name = entry.path.trim_end_matches(".afont.bin");
        let mut draw = |text: &str, size: f32, color: Color, baseline: f32| {
            for g in font.layout(text, size) {
                let [l, b, r, t] = g.rect;
                let [u0, v0, u1, v1] = g.uv;
                let (aw, ah) = (font.atlas_width as f32, font.atlas_height as f32);
                s.commands.spawn((
                    InScene,
                    Sprite {
                        image: image.clone(),
                        rect: Some(Rect::new(u0 * aw, v0 * ah, u1 * aw, v1 * ah)),
                        custom_size: Some(Vec2::new(r - l, t - b)),
                        color,
                        ..default()
                    },
                    Transform::from_xyz(left + (l + r) / 2.0, baseline + (b + t) / 2.0, 0.0),
                ));
            }
        };
        draw(
            &format!(
                "{name}  (ascent {:.2}, descent {:.2} em)",
                font.ascent, font.descent
            ),
            16.0,
            TEXT_WEAK,
            baseline,
        );
        baseline -= 16.0 * font.line_advance() + 16.0;
        for size in SIZES {
            for line in LINES {
                baseline -= size * font.ascent.max(0.5);
                draw(line, size, TEXT, baseline);
                baseline -= size * (font.line_advance() - font.ascent.max(0.5));
            }
            baseline -= 12.0;
        }
        baseline -= 32.0;
    }
}

/// The SDF atlas turned into white glyphs with antialiased alpha: distance (in red) around
/// `edge_value`, over about one atlas texel.
fn coverage(font: &sdk::Font, atlas: &sdk::Texture) -> Option<Image> {
    let level = atlas.levels.first()?;
    let soft = 0.5 / font.distance_range.max(1.0);
    let pixels = level
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let d = f32::from(p[0]) / 255.0;
            let a = ((d - font.edge_value) / (2.0 * soft) + 0.5).clamp(0.0, 1.0);
            [255, 255, 255, (a * 255.0).round() as u8]
        })
        .collect();
    Some(Image::new(
        Extent3d {
            width: level.width,
            height: level.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    ))
}
