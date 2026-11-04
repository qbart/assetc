//! Every mesh with its materials; skinned meshes play their animation clips and show their
//! locators.

use assetc_sdk as sdk;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use super::label_3d;
use crate::{ACCENT, Current, InScene, Orbit, Spawn, convert};

/// Every model is scaled to this height.
const HEIGHT: f32 = 2.0;
const SPACING: f32 = 3.0;

/// A skinned model playing its clips.
#[derive(Component)]
pub struct Player {
    animation: sdk::Animation,
    clip: usize,
    time: f32,
    /// The joint entities, in skeleton order, and their bind pose.
    joints: Vec<Entity>,
    bind: Vec<sdk::anim::Transform>,
    pose: Vec<sdk::anim::Transform>,
}

/// A locator, drawn as small axes.
#[derive(Component)]
pub struct LocatorMark;

pub fn spawn(s: &mut Spawn) -> Orbit {
    let Some(assets) = s.demo.assets.clone() else {
        return Orbit::looking_at(Vec3::ZERO, 6.0);
    };
    let entries: Vec<_> = assets.of_kind(sdk::Kind::Mesh).cloned().collect();
    let count = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let mesh = match assets.load::<sdk::Mesh>(entry.id) {
            Ok(mesh) => mesh,
            Err(e) => {
                warn!("{e}");
                continue;
            }
        };
        let x = (i as f32 - (count as f32 - 1.0) / 2.0) * SPACING;
        let (min, max) = (
            Vec3::from(mesh.bounds.aabb_min),
            Vec3::from(mesh.bounds.aabb_max),
        );
        let scale = HEIGHT / (max - min).max_element().max(1e-6);
        // Standing on the ground at x, centered.
        let center = (min + max) / 2.0;
        let offset = Vec3::new(x, 0.0, 0.0) - Vec3::new(center.x, min.y, center.z) * scale;
        let root = s
            .commands
            .spawn((
                InScene,
                Transform::from_translation(offset).with_scale(Vec3::splat(scale)),
                Visibility::default(),
            ))
            .id();
        spawn_model(s, &assets, &mesh, entry, root);
        let name = entry.path.trim_end_matches(".amesh.bin");
        label_3d(&mut s.commands, Vec3::new(x, -0.15, 0.0), name, 14.0, false);
    }
    let mut orbit = Orbit::looking_at(Vec3::Y * HEIGHT / 2.0, 3.0 + count as f32 * SPACING);
    orbit.yaw = 0.4;
    orbit
}

fn spawn_model(
    s: &mut Spawn,
    assets: &sdk::Assets,
    mesh: &sdk::Mesh,
    entry: &sdk::Entry,
    root: Entity,
) {
    // The skeleton: one entity per joint under its parent, in the bind pose.
    let bind: Vec<sdk::anim::Transform> = mesh.skeleton.iter().map(Into::into).collect();
    let joints: Vec<Entity> = bind
        .iter()
        .map(|t| {
            s.commands
                .spawn((InScene, transform(t), Visibility::default()))
                .id()
        })
        .collect();
    for (i, joint) in mesh.skeleton.iter().enumerate() {
        let parent = joint
            .parent
            .and_then(|p| joints.get(p).copied())
            .unwrap_or(root);
        s.commands.entity(parent).add_child(joints[i]);
    }
    let skin = mesh.is_skinned().then(|| {
        let matrices: Vec<Mat4> = mesh
            .skeleton
            .iter()
            .map(|j| Mat4::from_cols_array(&j.inverse_bind))
            .collect();
        SkinnedMesh {
            inverse_bindposes: s.bindposes.add(SkinnedMeshInverseBindposes::from(matrices)),
            joints: joints.clone(),
        }
    });

    // One Bevy mesh per submesh, sharing the vertices.
    for (index, sub) in mesh.submeshes.iter().enumerate() {
        let Some(indices) = mesh.level_indices(index, 0) else {
            continue;
        };
        let handle = s.meshes.add(bevy_mesh(mesh, indices));
        let material = sub
            .material
            .and_then(|m| mesh.materials.get(m as usize))
            .and_then(|&id| assets.load::<sdk::Material>(id).ok());
        let material = match material {
            Some(m) => {
                let textures = &mut s.textures;
                let images = &mut s.images;
                convert::material(&m, |id| textures.get(id, assets, images), None)
            }
            None => StandardMaterial::default(),
        };
        let mut part = s.commands.spawn((
            InScene,
            Mesh3d(handle),
            MeshMaterial3d(s.materials.add(material)),
            Transform::default(),
        ));
        if let Some(skin) = &skin {
            part.insert(skin.clone());
        }
        let part = part.id();
        s.commands.entity(root).add_child(part);
    }

    // Locators under their joint (or the model), labeled.
    for locator in &mesh.locators {
        let parent = locator
            .joint
            .and_then(|j| joints.get(j).copied())
            .unwrap_or(root);
        let mark = s
            .commands
            .spawn((
                InScene,
                LocatorMark,
                Transform {
                    translation: Vec3::from(locator.translation),
                    rotation: Quat::from_array(locator.rotation),
                    scale: Vec3::from(locator.scale),
                },
                Visibility::default(),
            ))
            .id();
        s.commands.entity(parent).add_child(mark);
        label_3d(&mut s.commands, mark, &locator.name, 11.0, true);
    }

    // The clips of `<name>.aanim.bin`, next to the mesh.
    let path = entry.path.replace(".amesh.bin", ".aanim.bin");
    if let Some(found) = assets.find(&path)
        && let Ok(animation) = assets.load::<sdk::Animation>(found.id)
        && !animation.clips.is_empty()
    {
        s.commands.entity(root).insert(Player {
            animation,
            clip: 0,
            time: 0.0,
            pose: bind.clone(),
            bind,
            joints,
        });
    }
}

fn transform(t: &sdk::anim::Transform) -> Transform {
    Transform {
        translation: Vec3::from(t.translation),
        rotation: Quat::from_array(t.rotation),
        scale: Vec3::from(t.scale),
    }
}

/// The vertices of `mesh` with `indices` as a Bevy mesh; skinned meshes carry their joints.
fn bevy_mesh(mesh: &sdk::Mesh, indices: &[u32]) -> Mesh {
    let v = &mesh.vertices;
    let mut out = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        v.iter().map(|v| v.position).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        v.iter().map(|v| v.normal).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        v.iter().map(|v| v.uv).collect::<Vec<_>>(),
    )
    .with_inserted_indices(Indices::U32(indices.to_vec()));
    if mesh.has_tangents() {
        out.insert_attribute(
            Mesh::ATTRIBUTE_TANGENT,
            v.iter().map(|v| v.tangent).collect::<Vec<_>>(),
        );
    }
    if mesh.is_skinned() && mesh.skin.len() == v.len() {
        out.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(mesh.skin.iter().map(|s| s.joints).collect()),
        );
        out.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            mesh.skin.iter().map(|s| s.weights).collect::<Vec<_>>(),
        );
    }
    out
}

/// Poses every skeleton at its clip's time: the bind pose, then the clip's channels.
pub fn animate(
    time: Res<Time>,
    mut players: Query<&mut Player>,
    mut joints: Query<&mut Transform>,
) {
    for mut player in &mut players {
        let player = &mut *player;
        let Some(clip) = player.animation.clips.get(player.clip) else {
            continue;
        };
        player.time = (player.time + time.delta_secs()) % clip.duration.max(1e-3);
        player.pose.clone_from(&player.bind);
        clip.apply(player.time, &mut player.pose);
        for (entity, pose) in player.joints.iter().zip(&player.pose) {
            if let Ok(mut t) = joints.get_mut(*entity) {
                *t = transform(pose);
            }
        }
    }
}

/// `Space` plays the next clip.
pub fn next_clip(
    keys: Res<ButtonInput<KeyCode>>,
    current: Res<Current>,
    mut players: Query<&mut Player>,
) {
    if current.shown != Some(super::Scene::Models) || !keys.just_pressed(KeyCode::Space) {
        return;
    }
    for mut player in &mut players {
        player.clip = (player.clip + 1) % player.animation.clips.len();
        player.time = 0.0;
        let name = &player.animation.clips[player.clip].name;
        info!("clip {name}");
    }
}

pub fn draw_locators(mut gizmos: Gizmos, marks: Query<&GlobalTransform, With<LocatorMark>>) {
    for mark in &marks {
        let (_, rotation, at) = mark.to_scale_rotation_translation();
        let size = 0.12;
        gizmos.line(
            at,
            at + rotation * Vec3::X * size,
            Color::srgb(0.9, 0.28, 0.3),
        );
        gizmos.line(
            at,
            at + rotation * Vec3::Y * size,
            Color::srgb(0.27, 0.65, 0.35),
        );
        gizmos.line(
            at,
            at + rotation * Vec3::Z * size,
            Color::srgb(0.24, 0.56, 0.93),
        );
        gizmos.sphere(Isometry3d::from_translation(at), 0.015, ACCENT);
    }
}
