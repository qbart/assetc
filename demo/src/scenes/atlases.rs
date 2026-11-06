//! Every atlas: its frames with their names; numbered frames also play as a flipbook.

use assetc as sdk;
use bevy::prelude::*;

use super::text_2d;
use crate::{BAR, InScene, Spawn, TEXT, TEXT_WEAK};

const CELL: f32 = 96.0;
const GAP: f32 = 24.0;
const FPS: f32 = 12.0;

/// A sprite cycling through atlas frames.
#[derive(Component)]
pub struct Flipbook {
    frames: Vec<Rect>,
    time: f32,
}

pub fn spawn(s: &mut Spawn) {
    let Some(assets) = s.demo.assets.clone() else {
        return;
    };
    let (width, height) = (s.window.width(), s.window.height());
    let left = -width / 2.0 + GAP * 2.0;
    let columns = (((width - GAP * 4.0) / (CELL + GAP)).floor() as usize).max(1);
    let mut top = height / 2.0 - BAR - GAP;
    for entry in assets.of_kind(sdk::Kind::Atlas) {
        let Ok(atlas) = assets.load::<sdk::Atlas>(entry.id) else {
            continue;
        };
        let Some(image) = s.textures.get(atlas.texture, &assets, &mut s.images) else {
            continue;
        };
        let name = entry.path.trim_end_matches(".aatlas.bin");
        s.commands.spawn((
            InScene,
            Text2d::new(format!(
                "{name}  ({} frames, {} x {})",
                atlas.frames.len(),
                atlas.width,
                atlas.height
            )),
            crate::font(15.0),
            TextColor(TEXT),
            bevy::sprite::Anchor::TOP_LEFT,
            Transform::from_xyz(left, top, 1.0),
        ));
        top -= 34.0;
        let rect = |f: &sdk::atlas::Frame| {
            Rect::new(
                f.x as f32,
                f.y as f32,
                (f.x + f.width) as f32,
                (f.y + f.height) as f32,
            )
        };
        let fit = |f: &sdk::atlas::Frame| {
            let size = Vec2::new(f.width as f32, f.height as f32);
            size * (CELL / size.max_element().max(1.0)).min(2.0)
        };
        for (i, frame) in atlas.frames.iter().enumerate() {
            let (col, row) = (i % columns, i / columns);
            let center = Vec2::new(
                left + col as f32 * (CELL + GAP) + CELL / 2.0,
                top - row as f32 * (CELL + GAP + 20.0) - CELL / 2.0,
            );
            s.commands.spawn((
                InScene,
                Sprite {
                    image: image.clone(),
                    rect: Some(rect(frame)),
                    custom_size: Some(fit(frame)),
                    ..default()
                },
                Transform::from_translation(center.extend(0.0)),
            ));
            text_2d(
                &mut s.commands,
                center - Vec2::Y * (CELL / 2.0 + 4.0),
                &frame.name,
                12.0,
                TEXT_WEAK,
            );
        }
        let rows = atlas.frames.len().div_ceil(columns);
        top -= rows as f32 * (CELL + GAP + 20.0);
        // Frames named like `spark_01`, `spark_02`, … also play in order.
        let numbered: Vec<&sdk::atlas::Frame> = atlas
            .frames
            .iter()
            .filter(|f| f.name.ends_with(|c: char| c.is_ascii_digit()))
            .collect();
        if numbered.len() > 1 {
            let center = Vec2::new(left + CELL, top - CELL);
            s.commands.spawn((
                InScene,
                Flipbook {
                    frames: numbered.iter().map(|f| rect(f)).collect(),
                    time: 0.0,
                },
                Sprite {
                    image: image.clone(),
                    rect: Some(rect(numbered[0])),
                    custom_size: Some(fit(numbered[0]) * 2.0),
                    ..default()
                },
                Transform::from_translation(center.extend(0.0)),
            ));
            text_2d(
                &mut s.commands,
                center - Vec2::Y * (CELL + 4.0),
                format!("flipbook, {FPS} fps"),
                12.0,
                TEXT_WEAK,
            );
            top -= CELL * 2.0 + GAP + 20.0;
        }
        top -= GAP;
    }
}

pub fn flip(time: Res<Time>, mut books: Query<(&mut Flipbook, &mut Sprite)>) {
    for (mut book, mut sprite) in &mut books {
        book.time += time.delta_secs();
        let i = (book.time * FPS) as usize % book.frames.len();
        sprite.rect = Some(book.frames[i]);
    }
}
