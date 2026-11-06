//! Every 2D texture as a tile with its path.

use assetc as sdk;
use bevy::prelude::*;

use super::text_2d;
use crate::{BAR, InScene, Spawn, TEXT_WEAK};

const TILE: f32 = 160.0;
const GAP: f32 = 28.0;
const CAPTION: f32 = 30.0;

pub fn spawn(s: &mut Spawn) {
    let Some(assets) = s.demo.assets.clone() else {
        return;
    };
    let (width, height) = (s.window.width(), s.window.height());
    let columns = (((width - GAP) / (TILE + GAP)).floor() as usize).max(1);
    let left = -(columns as f32 * (TILE + GAP) - GAP) / 2.0;
    let top = height / 2.0 - BAR - GAP;
    let mut shown = 0;
    for entry in assets.of_kind(sdk::Kind::Texture) {
        let Ok(texture) = assets.load::<sdk::Texture>(entry.id) else {
            continue;
        };
        // Cubemaps light the 3D scenes; volumes have no flat picture.
        if texture.is_cubemap() || texture.is_volume() {
            continue;
        }
        let Some(image) = s.textures.get(entry.id, &assets, &mut s.images) else {
            continue;
        };
        let (col, row) = (shown % columns, shown / columns);
        shown += 1;
        let center = Vec2::new(
            left + col as f32 * (TILE + GAP) + TILE / 2.0,
            top - row as f32 * (TILE + GAP + CAPTION) - TILE / 2.0,
        );
        let aspect = texture.width as f32 / texture.height as f32;
        let size = if aspect >= 1.0 {
            Vec2::new(TILE, TILE / aspect)
        } else {
            Vec2::new(TILE * aspect, TILE)
        };
        s.commands.spawn((
            InScene,
            Sprite {
                image,
                custom_size: Some(size),
                ..default()
            },
            Transform::from_translation(center.extend(0.0)),
        ));
        let name = entry.path.trim_end_matches(".ktx2");
        text_2d(
            &mut s.commands,
            center - Vec2::new(0.0, TILE / 2.0 + 6.0),
            format!("{name}\n{} x {}", texture.width, texture.height),
            12.0,
            TEXT_WEAK,
        );
    }
}
