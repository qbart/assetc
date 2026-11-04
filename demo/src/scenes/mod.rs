//! The demo's scenes, one module each; see docs/features/demo.md#scenes.

mod atlases;
mod data;
mod fonts;
mod graphs;
mod materials;
mod models;
mod textures;

use bevy::prelude::*;

use crate::{Camera3dMarker, InScene, Orbit, Spawn, TEXT, TEXT_WEAK, font};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scene {
    Textures,
    Materials,
    Models,
    Atlases,
    Fonts,
    Data,
    Graphs,
}

impl Scene {
    pub const ALL: [Self; 7] = [
        Self::Textures,
        Self::Materials,
        Self::Models,
        Self::Atlases,
        Self::Fonts,
        Self::Data,
        Self::Graphs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Textures => "Textures",
            Self::Materials => "Materials",
            Self::Models => "Models",
            Self::Atlases => "Atlases",
            Self::Fonts => "Fonts",
            Self::Data => "Data",
            Self::Graphs => "Graphs",
        }
    }

    pub fn is_3d(self) -> bool {
        matches!(self, Self::Materials | Self::Models)
    }
}

/// Per-frame systems of the scenes.
pub struct ScenesPlugin;

impl Plugin for ScenesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                models::animate,
                models::next_clip,
                models::draw_locators,
                atlases::flip,
                graphs::draw,
            ),
        );
    }
}

/// Spawns `scene`; a 3D scene returns where its camera looks.
pub fn spawn(scene: Scene, s: &mut Spawn) -> Option<Orbit> {
    match scene {
        Scene::Textures => {
            textures::spawn(s);
            None
        }
        Scene::Materials => Some(materials::spawn(s)),
        Scene::Models => Some(models::spawn(s)),
        Scene::Atlases => {
            atlases::spawn(s);
            None
        }
        Scene::Fonts => {
            fonts::spawn(s);
            None
        }
        Scene::Data => {
            data::spawn(s);
            None
        }
        Scene::Graphs => {
            graphs::spawn(s);
            None
        }
    }
}

/// A label kept on screen over a point of the 3D scene, or over an entity.
#[derive(Component)]
pub enum Label3d {
    At(Vec3),
    On(Entity),
}

/// A UI label over `at` (a point or an entity) in the 3D scene.
pub fn label_3d(
    commands: &mut Commands,
    at: impl Into<Label3dAt>,
    text: impl Into<String>,
    size: f32,
    weak: bool,
) {
    commands.spawn((
        InScene,
        Into::<Label3dAt>::into(at).0,
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        Text::new(text),
        font(size),
        TextColor(if weak { TEXT_WEAK } else { TEXT }),
    ));
}

/// World-space text in a 2D scene, its top centered on `at`.
pub fn text_2d(
    commands: &mut Commands,
    at: Vec2,
    text: impl Into<String>,
    size: f32,
    color: Color,
) {
    commands.spawn((
        InScene,
        Text2d::new(text),
        font(size),
        TextColor(color),
        bevy::sprite::Anchor::TOP_CENTER,
        Transform::from_translation(at.extend(1.0)),
    ));
}

/// Where a [`label_3d`] goes.
pub struct Label3dAt(Label3d);

impl From<Vec3> for Label3dAt {
    fn from(at: Vec3) -> Self {
        Self(Label3d::At(at))
    }
}

impl From<Entity> for Label3dAt {
    fn from(entity: Entity) -> Self {
        Self(Label3d::On(entity))
    }
}

/// Moves each [`Label3d`] to where its point is on screen, centered below it.
pub fn place_labels(
    camera: Single<(&Camera, &GlobalTransform), With<Camera3dMarker>>,
    mut labels: Query<(&Label3d, &mut Node, &mut Visibility, &ComputedNode)>,
    targets: Query<&GlobalTransform>,
) {
    let (camera, transform) = *camera;
    for (label, mut node, mut visibility, computed) in &mut labels {
        let at = match label {
            Label3d::At(at) => Some(*at),
            Label3d::On(entity) => targets.get(*entity).ok().map(|t| t.translation()),
        };
        match at.map(|at| camera.world_to_viewport(transform, at)) {
            Some(Ok(p)) if camera.is_active => {
                let size = computed.size() * computed.inverse_scale_factor();
                node.left = Val::Px(p.x - size.x / 2.0);
                node.top = Val::Px(p.y);
                *visibility = Visibility::Inherited;
            }
            _ => *visibility = Visibility::Hidden,
        }
    }
}
