//! Every material on a sphere, lit by the sky.

use assetc as sdk;
use bevy::prelude::*;

use super::label_3d;
use crate::{InScene, Orbit, Spawn, convert};

const SPACING: f32 = 2.4;
const PER_ROW: usize = 4;

pub fn spawn(s: &mut Spawn) -> Orbit {
    let Some(assets) = s.demo.assets.clone() else {
        return Orbit::looking_at(Vec3::ZERO, 6.0);
    };
    let mut sphere = Sphere::new(0.9).mesh().uv(64, 32);
    // Tangents, for normal and depth maps.
    let _ = sphere.generate_tangents();
    let sphere = s.meshes.add(sphere);
    let entries: Vec<_> = assets.of_kind(sdk::Kind::Material).cloned().collect();
    let rows = entries.len().div_ceil(PER_ROW).max(1);
    let columns = entries.len().clamp(1, PER_ROW);
    for (i, entry) in entries.iter().enumerate() {
        let Ok(m) = assets.load::<sdk::Material>(entry.id) else {
            continue;
        };
        let depth = (m.textures.height != 0)
            .then(|| assets.load::<sdk::Texture>(m.textures.height).ok())
            .flatten()
            .and_then(|t| convert::depth(&t))
            .map(|image| s.images.add(image));
        let textures = &mut s.textures;
        let images = &mut s.images;
        let material = convert::material(&m, |id| textures.get(id, &assets, images), depth);
        let (col, row) = (i % PER_ROW, i / PER_ROW);
        let at = Vec3::new(
            (col as f32 - (columns as f32 - 1.0) / 2.0) * SPACING,
            ((rows as f32 - 1.0) / 2.0 - row as f32) * SPACING,
            0.0,
        );
        s.commands.spawn((
            InScene,
            Mesh3d(sphere.clone()),
            MeshMaterial3d(s.materials.add(material)),
            Transform::from_translation(at),
        ));
        let name = entry.path.trim_end_matches(".amat.bin");
        label_3d(&mut s.commands, at - Vec3::Y * 1.05, name, 13.0, true);
    }
    Orbit::looking_at(Vec3::ZERO, 3.5 + rows.max(columns) as f32 * SPACING)
}
