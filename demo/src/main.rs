//! Shows every sample the way a game would load it: through the `assetc` SDK only. See
//! docs/features/demo.md.

mod convert;
mod scenes;
mod scroll;

use std::collections::HashMap;

use assetc as sdk;
use bevy::core_pipeline::Skybox;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::GeneratedEnvironmentMapLight;
use bevy::prelude::*;

use scenes::Scene;

/// The GUI's look: background, panels, text and accent.
pub const BG: Color = Color::srgb(0.157, 0.157, 0.169);
pub const PANEL: Color = Color::srgb(0.118, 0.118, 0.129);
pub const TEXT: Color = Color::srgb(0.902, 0.902, 0.910);
pub const TEXT_WEAK: Color = Color::srgb(0.553, 0.553, 0.580);
pub const ACCENT: Color = Color::srgb(0.941, 0.533, 0.243);

/// Height of the tab bar and the status line, in logical pixels.
pub const BAR: f32 = 36.0;

const DEFAULT_PATH: &str = "tmp/build/assetc.pak";
/// Skybox and image-based lighting strength, cd/m².
const SKY_BRIGHTNESS: f32 = 1000.0;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PATH.to_owned());
    let (assets, error) = match sdk::Assets::open(&path) {
        Ok(assets) => (Some(std::sync::Arc::new(assets)), None),
        Err(e) => (None, Some(e.to_string())),
    };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "assetc demo".into(),
                resolution: (1400, 860).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(BG))
        .insert_resource(Demo {
            assets,
            error,
            path,
            sky: None,
        })
        .init_resource::<Textures>()
        .init_resource::<Current>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (keys, switch_scene, (orbit, scenes::place_labels)).chain(),
        )
        .add_plugins((scenes::ScenesPlugin, scroll::ScrollPlugin))
        .run();
}

/// The opened assets, or why they could not be opened.
#[derive(Resource)]
pub struct Demo {
    pub assets: Option<std::sync::Arc<sdk::Assets>>,
    pub error: Option<String>,
    pub path: String,
    /// The pack's first cubemap, lighting the 3D scenes.
    pub sky: Option<Handle<Image>>,
}

/// Textures uploaded so far, by asset ID: each is converted once and shared.
#[derive(Resource, Default)]
pub struct Textures(pub HashMap<u64, Option<Handle<Image>>>);

impl Textures {
    /// The texture `id` as a Bevy image (`None` for `0`, a missing ID or a bad file).
    pub fn get(
        &mut self,
        id: u64,
        assets: &sdk::Assets,
        images: &mut Assets<Image>,
    ) -> Option<Handle<Image>> {
        if id == 0 {
            return None;
        }
        self.0
            .entry(id)
            .or_insert_with(|| match assets.load::<sdk::Texture>(id) {
                Ok(texture) => Some(images.add(convert::image(&texture))),
                Err(e) => {
                    warn!("{e}");
                    None
                }
            })
            .clone()
    }
}

/// The scene shown, and the one asked for.
#[derive(Resource)]
pub struct Current {
    pub shown: Option<Scene>,
    pub wanted: Scene,
}

impl Default for Current {
    fn default() -> Self {
        Self {
            shown: None,
            wanted: Scene::Textures,
        }
    }
}

/// Everything a scene spawned; despawned when another scene shows.
#[derive(Component)]
pub struct InScene;

#[derive(Component)]
struct Tab(Scene);

#[derive(Component)]
struct Status;

/// The 3D camera's orbit around `target`.
#[derive(Component)]
pub struct Orbit {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Orbit {
    pub fn looking_at(target: Vec3, distance: f32) -> Self {
        Self {
            target,
            yaw: 0.0,
            pitch: 0.25,
            distance,
        }
    }

    fn transform(&self) -> Transform {
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        Transform::from_translation(self.target + rotation * Vec3::new(0.0, 0.0, self.distance))
            .with_rotation(rotation)
    }
}

#[derive(Component)]
pub struct Camera3dMarker;

#[derive(Component)]
pub struct Camera2dMarker;

fn setup(mut commands: Commands, mut demo: ResMut<Demo>, mut images: ResMut<Assets<Image>>) {
    // The sky: the pack's first cubemap, by path.
    if let Some(assets) = &demo.assets {
        let cubemap = assets
            .of_kind(sdk::Kind::Texture)
            .filter_map(|e| assets.load::<sdk::Texture>(e.id).ok())
            .find(sdk::Texture::is_cubemap);
        demo.sky = cubemap.map(|t| images.add(convert::image(&t)));
    }
    let mut camera3d = commands.spawn((
        Camera3dMarker,
        Camera3d::default(),
        Camera {
            order: 0,
            ..default()
        },
        Tonemapping::AcesFitted,
        Orbit::looking_at(Vec3::ZERO, 6.0),
        Transform::default(),
    ));
    if let Some(sky) = &demo.sky {
        camera3d.insert((
            Skybox {
                image: Some(sky.clone()),
                brightness: SKY_BRIGHTNESS,
                ..default()
            },
            GeneratedEnvironmentMapLight {
                environment_map: sky.clone(),
                intensity: SKY_BRIGHTNESS,
                ..default()
            },
        ));
    } else {
        commands.spawn((
            DirectionalLight {
                illuminance: 6000.0,
                ..default()
            },
            Transform::default().looking_to(Vec3::new(-0.5, -0.7, -1.0), Vec3::Y),
        ));
    }
    commands.spawn((
        Camera2dMarker,
        Camera2d,
        Camera {
            order: 1,
            ..default()
        },
        IsDefaultUiCamera,
    ));

    // Tabs along the top, the status line along the bottom.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                height: Val::Px(BAR),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                column_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|bar| {
            for (i, scene) in Scene::ALL.into_iter().enumerate() {
                bar.spawn((
                    Tab(scene),
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                ))
                .observe(move |_: On<PointerClick>, mut current: ResMut<Current>| {
                    current.wanted = scene;
                })
                .with_child((
                    Text::new(format!("{} {}", i + 1, scene.name())),
                    font(15.0),
                    TextColor(TEXT),
                    Pickable::IGNORE,
                ));
            }
        });
    let status = match (&demo.assets, &demo.error) {
        (Some(assets), _) => format!(
            "{} assets from {} ({})",
            assets.entries().len(),
            demo.path,
            if assets.pack().is_some() {
                "pack"
            } else {
                "directory"
            }
        ),
        (None, Some(e)) => format!("{}: {e}. Run: cargo run -- pack", demo.path),
        (None, None) => String::new(),
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                height: Val::Px(BAR - 8.0),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(12.0)),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_child((Status, Text::new(status), font(13.0), TextColor(TEXT_WEAK)));
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut current: ResMut<Current>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (key, scene) in digits.into_iter().zip(Scene::ALL) {
        if keys.just_pressed(key) {
            current.wanted = scene;
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let i = Scene::ALL
            .iter()
            .position(|s| *s == current.wanted)
            .unwrap_or(0);
        let n = Scene::ALL.len();
        let back = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        current.wanted = Scene::ALL[if back { (i + n - 1) % n } else { (i + 1) % n }];
    }
}

/// Text of `size` logical pixels in the default font.
pub fn font(size: f32) -> TextFont {
    TextFont {
        font_size: bevy::text::FontSize::Px(size),
        ..default()
    }
}

/// What spawning a scene needs.
#[derive(SystemParam)]
pub struct Spawn<'w, 's> {
    pub commands: Commands<'w, 's>,
    pub images: ResMut<'w, Assets<Image>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub bindposes: ResMut<'w, Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>,
    pub textures: ResMut<'w, Textures>,
    pub demo: Res<'w, Demo>,
    pub window: Single<'w, 's, &'static Window>,
}

#[allow(clippy::type_complexity)]
fn switch_scene(
    mut current: ResMut<Current>,
    mut spawn: Spawn,
    // Roots only: despawning one takes its children along.
    old: Query<Entity, (With<InScene>, Without<ChildOf>)>,
    mut tabs: Query<(&Tab, &mut BackgroundColor)>,
    mut camera3d: Single<
        (&mut Camera, &mut Orbit),
        (With<Camera3dMarker>, Without<Camera2dMarker>),
    >,
    mut camera2d: Single<
        (&mut Camera, &mut Transform),
        (With<Camera2dMarker>, Without<Camera3dMarker>),
    >,
) {
    if current.shown == Some(current.wanted) {
        return;
    }
    let scene = current.wanted;
    current.shown = Some(scene);
    for entity in &old {
        spawn.commands.entity(entity).despawn();
    }
    for (tab, mut color) in &mut tabs {
        color.0 = if tab.0 == scene {
            ACCENT.with_alpha(0.35)
        } else {
            PANEL
        };
    }
    let (camera, orbit) = &mut *camera3d;
    camera.is_active = scene.is_3d();
    let (camera2d, transform2d) = &mut *camera2d;
    // The 2D camera clears the screen when it is the only one drawing.
    camera2d.clear_color = if scene.is_3d() {
        ClearColorConfig::None
    } else {
        ClearColorConfig::Custom(BG)
    };
    transform2d.translation = Vec3::ZERO;
    if spawn.demo.assets.is_none() {
        return;
    }
    if let Some(view) = scenes::spawn(scene, &mut spawn) {
        **orbit = view;
    }
}

fn orbit(
    current: Res<Current>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut camera: Single<(&mut Orbit, &mut Transform), With<Camera3dMarker>>,
) {
    if !current.shown.is_some_and(Scene::is_3d) {
        return;
    }
    let (orbit, transform) = &mut *camera;
    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.005).clamp(-1.4, 1.4);
    }
    if scroll.delta.y != 0.0 {
        orbit.distance = (orbit.distance * 0.9f32.powf(scroll.delta.y)).clamp(0.5, 200.0);
    }
    **transform = orbit.transform();
}
