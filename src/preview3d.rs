//! The canvas' 3D mode: the selected image on a rotating, lit shape, rendered
//! behind the egui overlay. See docs/features/canvas.md#3d.

use std::path::PathBuf;
use std::time::Instant;

use bevy::asset::{AssetPath, LoadState, RenderAssetUsages};
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::gltf::Gltf;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{GeneratedEnvironmentMapLight, Skybox};
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{CylinderMeshBuilder, Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use bevy::transform::TransformSystems;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};

use crate::theme::BG_CENTER;

/// Asset source for the project's input directory, registered at startup; models load from it.
pub const INPUT_SOURCE: &str = "input";
/// Longest side of a fitted model's bounding box, in units.
const MODEL_SIZE: f32 = 1.5;

pub struct Preview3dPlugin;

impl Plugin for Preview3dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Preview3d>()
            .insert_resource(GlobalAmbientLight {
                brightness: AMBIENT_BRIGHTNESS,
                ..default()
            })
            .init_resource::<LoadedModel>()
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                (
                    (sync, sync_model).before(TransformSystems::Propagate),
                    fit_model.after(TransformSystems::Propagate),
                ),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Box,
    Sphere,
    Cylinder,
    Torus,
}

impl Shape {
    pub const ALL: [Self; 4] = [Self::Box, Self::Sphere, Self::Cylinder, Self::Torus];

    pub fn name(self) -> &'static str {
        match self {
            Self::Box => "Box",
            Self::Sphere => "Sphere",
            Self::Cylinder => "Cylinder",
            Self::Torus => "Torus",
        }
    }
}

/// Orbit camera around the shape's center.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Orbit {
    /// Radians around the vertical axis.
    pub yaw: f32,
    /// Radians above the horizon.
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 30f32.to_radians(),
            pitch: 20f32.to_radians(),
            distance: 3.5,
        }
    }
}

impl Orbit {
    const PITCH_LIMIT: f32 = 85.0;
    const DISTANCE: std::ops::RangeInclusive<f32> = 1.5..=10.0;

    /// Drag by `delta` points: 0.3° per point, dragging right/down turns the view right/up.
    pub fn drag(&mut self, delta: Vec2) {
        let step = 0.3f32.to_radians();
        let limit = Self::PITCH_LIMIT.to_radians();
        self.yaw -= delta.x * step;
        self.pitch = (self.pitch + delta.y * step).clamp(-limit, limit);
    }

    /// Divides the distance by `factor` (> 1 moves closer).
    pub fn dolly(&mut self, factor: f32) {
        self.distance =
            (self.distance / factor).clamp(*Self::DISTANCE.start(), *Self::DISTANCE.end());
    }

    pub fn transform(&self) -> Transform {
        let offset = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0)
            * Vec3::new(0.0, 0.0, self.distance);
        Transform::from_translation(offset).looking_at(Vec3::ZERO, Vec3::Y)
    }
}

/// A texture for the shape: RGBA8 pixels (sRGB), rows top to bottom.
pub struct Upload {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// Alpha-blend instead of drawing opaque.
    pub blend: bool,
}

/// Written by the UI every frame; applied by [`sync`].
#[derive(Resource, Default)]
pub struct Preview3d {
    /// Canvas area as (position, size) in physical pixels; `None` hides the 3D view.
    pub viewport: Option<(UVec2, UVec2)>,
    pub shape: Shape,
    /// Shape for environment maps, kept apart from `shape`; `None` is Sphere.
    pub environment_shape: Option<Shape>,
    pub orbit: Orbit,
    /// Being orbited: auto-rotation pauses.
    pub dragging: bool,
    /// A new texture to show, taken by [`sync`].
    pub upload: Option<Upload>,
    /// Show the environment map (skybox + lighting) instead of the textured shape.
    pub environment: bool,
    /// Show the color cube (unlit, vertex colors) instead of the shape.
    pub colors: bool,
    /// Show the untransformed cube instead of the LUT's.
    pub colors_before: bool,
    /// New color cube meshes (before, after), taken by [`sync`].
    pub color_cube: Option<(Mesh, Mesh)>,
    /// A new environment map, taken by [`sync`].
    pub cubemap: Option<CubemapUpload>,
    /// A glTF model to show instead of the shape: input-relative path and a generation; a
    /// new generation of the same file reads it from disk again.
    pub model: Option<(PathBuf, u64)>,
    /// How the requested model is doing, written by [`sync_model`].
    pub model_status: ModelStatus,
    /// The model's animation to play, by index in the file; `None` is the rest pose.
    pub animation: Option<usize>,
    /// A compiled mesh to show instead of the shape, taken by [`sync`].
    pub compiled: Option<crate::amesh::Preview>,
    /// Show the last compiled mesh uploaded.
    pub compiled_shown: bool,
    /// Color the compiled mesh by meshlet rather than plain gray.
    pub meshlet_colors: bool,
}

#[derive(Clone, Default, PartialEq, Debug)]
pub enum ModelStatus {
    #[default]
    Idle,
    /// Loading or fitting since.
    Loading(Instant),
    Ready,
    Failed(String),
}

impl Preview3d {
    pub fn environment_shape(&self) -> Shape {
        self.environment_shape.unwrap_or(Shape::Sphere)
    }
}

/// Six square RGBA8 sRGB faces in cube layer order (+X, −X, +Y, −Y, +Z, −Z), concatenated.
pub struct CubemapUpload {
    pub size: u32,
    pub pixels: Vec<u8>,
}

#[derive(Component)]
struct PreviewCamera;

#[derive(Component)]
struct PreviewShape;

#[derive(Resource)]
struct Scene {
    /// Same order as [`Shape::ALL`].
    meshes: [Handle<Mesh>; 4],
    /// Textured with the shown image.
    material: Handle<StandardMaterial>,
    /// Mirror-like, to show an environment map's lighting.
    chrome: Handle<StandardMaterial>,
    /// Unlit, showing vertex colors.
    unlit: Handle<StandardMaterial>,
    /// Untransformed and LUT color cubes.
    color_cube: Option<(Handle<Mesh>, Handle<Mesh>)>,
    /// A compiled mesh: gray, and colored by meshlet.
    compiled: Option<[Handle<Mesh>; 2]>,
    /// Lit, showing vertex colors.
    vertex_colors: Handle<StandardMaterial>,
    cubemap: Option<Handle<Image>>,
}

/// Skybox and image-based lighting strength for environment maps, cd/m².
const ENVIRONMENT_BRIGHTNESS: f32 = 1000.0;
const AMBIENT_BRIGHTNESS: f32 = 600.0;

/// (normal, right, up) per cube face, sides first; right × up = normal, so corners
/// listed bottom-left, bottom-right, top-right, top-left are counter-clockwise from outside.
const CUBE_FACES: [(Vec3, Vec3, Vec3); 6] = [
    (Vec3::Z, Vec3::X, Vec3::Y),
    (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
    (Vec3::X, Vec3::NEG_Z, Vec3::Y),
    (Vec3::NEG_X, Vec3::Z, Vec3::Y),
    (Vec3::Y, Vec3::X, Vec3::NEG_Z),
    (Vec3::NEG_Y, Vec3::X, Vec3::Z),
];

/// Cube with the whole image on each face, upright on the sides (Bevy's `Cuboid` turns some
/// faces); see docs/features/canvas.md#scene.
fn cube(size: f32) -> Mesh {
    let half = size / 2.0;
    let (mut positions, mut normals, mut uvs, mut indices) = (vec![], vec![], vec![], vec![]);
    for (normal, right, up) in CUBE_FACES {
        let base = positions.len() as u32;
        // Bottom-left, bottom-right, top-right, top-left; UV v grows downward.
        for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            positions.push(((normal + right * x + up * y) * half).to_array());
            normals.push(normal.to_array());
            uvs.push([(x + 1.0) / 2.0, (1.0 - y) / 2.0]);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// The RGB cube transformed by `map`: red along +X, green +Y, blue +Z, each face an
/// `n` × `n` grid colored with `map(input)`; see docs/features/lut.md#cube.
pub fn color_cube(n: usize, map: impl Fn([f32; 3]) -> [f32; 3]) -> Mesh {
    const SIZE: f32 = 1.2;
    let (mut positions, mut colors, mut indices) = (vec![], vec![], vec![]);
    for (normal, right, up) in CUBE_FACES {
        let base = positions.len() as u32;
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f32 / (n - 1) as f32, j as f32 / (n - 1) as f32);
                let p = (normal + right * (2.0 * x - 1.0) + up * (2.0 * y - 1.0)) * 0.5;
                let input = (p + 0.5).to_array();
                let [r, g, b] = map(input);
                positions.push((p * SIZE).to_array());
                // The LUT's output is display (sRGB) encoded; vertex colors are linear.
                colors.push(Color::srgb(r, g, b).to_linear().to_f32_array());
            }
        }
        let n = n as u32;
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = base + j * n + i;
                indices.extend([a, a + 1, a + n + 1, a, a + n + 1, a + n]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

type ShapeQuery = (
    &'static mut Mesh3d,
    &'static mut MeshMaterial3d<StandardMaterial>,
    &'static mut Transform,
    &'static mut Visibility,
);
type ShapeFilter = (
    With<PreviewShape>,
    Without<PreviewCamera>,
    Without<DirectionalLight>,
);

/// Auto-rotation, radians per second.
const SPIN: f32 = 20.0 * std::f32::consts::PI / 180.0;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let [r, g, b, _] = BG_CENTER.to_array();
    let meshes = [
        meshes.add(cube(1.2)),
        // Bevy's UV sphere has its poles on Z; stand it up so rows of the image run around.
        meshes.add(
            Sphere::new(0.75)
                .mesh()
                .uv(48, 24)
                .rotated_by(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ),
        meshes.add(CylinderMeshBuilder::new(0.6, 1.4, 48)),
        meshes.add(Torus::new(0.35, 0.85)),
    ];
    let material = materials.add(StandardMaterial {
        perceptual_roughness: 0.8,
        metallic: 0.0,
        ..default()
    });
    let chrome = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        metallic: 1.0,
        perceptual_roughness: 0.2,
        ..default()
    });

    let unlit = materials.add(StandardMaterial {
        unlit: true,
        ..default()
    });
    let vertex_colors = materials.add(StandardMaterial {
        perceptual_roughness: 0.8,
        ..default()
    });

    commands
        .spawn((
            PreviewCamera,
            Camera3d::default(),
            Camera {
                // Before the egui camera, which draws the overlay on top.
                order: -1,
                is_active: false,
                clear_color: ClearColorConfig::Custom(Color::srgb_u8(r, g, b)),
                ..default()
            },
            // Colors as stored, like the 2D view.
            bevy::render::view::Tonemapping::None,
            Projection::Perspective(PerspectiveProjection {
                fov: 45f32.to_radians(),
                ..default()
            }),
            Orbit::default().transform(),
        ))
        .with_child((
            // Fixed relative to the view: from the upper left, behind the camera.
            DirectionalLight {
                illuminance: 4000.0,
                ..default()
            },
            Transform::default().looking_to(Vec3::new(0.5, -0.6, -1.0), Vec3::Y),
        ));
    commands.spawn((
        PreviewShape,
        Mesh3d(meshes[0].clone()),
        MeshMaterial3d(material.clone()),
        Transform::default(),
    ));
    commands.insert_resource(Scene {
        meshes,
        material,
        chrome,
        unlit,
        color_cube: None,
        compiled: None,
        vertex_colors,
        cubemap: None,
    });
}

#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    mut preview: ResMut<Preview3d>,
    mut scene: ResMut<Scene>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut camera: Single<(Entity, &mut Camera, &mut Transform), With<PreviewCamera>>,
    mut overlay: Query<&mut Camera, (With<Camera2d>, Without<PreviewCamera>)>,
    mut shape: Single<ShapeQuery, ShapeFilter>,
    mut lights: Query<&mut Visibility, With<DirectionalLight>>,
    mut environment_on: Local<bool>,
) {
    let active = preview.viewport.is_some();
    let (camera_entity, camera, camera_transform) = &mut *camera;
    let camera_entity = *camera_entity;
    camera.is_active = active;
    camera.viewport = preview
        .viewport
        .map(|(physical_position, physical_size)| Viewport {
            physical_position,
            physical_size,
            ..default()
        });
    **camera_transform = preview.orbit.transform();
    // The overlay keeps the 3D image instead of clearing over it.
    for mut overlay in &mut overlay {
        overlay.clear_color = if active {
            ClearColorConfig::None
        } else {
            ClearColorConfig::Default
        };
    }

    if let Some(upload) = preview.cubemap.take() {
        let mut image = Image::new(
            Extent3d {
                width: upload.size,
                height: upload.size,
                depth_or_array_layers: 6,
            },
            TextureDimension::D2,
            upload.pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_view_descriptor = Some(TextureViewDescriptor {
            dimension: Some(TextureViewDimension::Cube),
            ..default()
        });
        if let Some(old) = scene.cubemap.replace(images.add(image)) {
            images.remove(&old);
        }
        // Re-insert so the lighting is generated from the new map.
        *environment_on = false;
    }

    let environment = active && preview.environment && scene.cubemap.is_some();
    if environment != *environment_on {
        let mut camera = commands.entity(camera_entity);
        match (&scene.cubemap, environment) {
            (Some(cubemap), true) => {
                camera.insert((
                    Skybox {
                        image: Some(cubemap.clone()),
                        brightness: ENVIRONMENT_BRIGHTNESS,
                        ..default()
                    },
                    GeneratedEnvironmentMapLight {
                        environment_map: cubemap.clone(),
                        intensity: ENVIRONMENT_BRIGHTNESS,
                        ..default()
                    },
                ));
            }
            _ => {
                camera.remove::<(Skybox, GeneratedEnvironmentMapLight)>();
            }
        }
        // The environment is the only light.
        for mut visibility in &mut lights {
            *visibility = if environment {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
        ambient.brightness = if environment { 0.0 } else { AMBIENT_BRIGHTNESS };
        *environment_on = environment;
    }

    if let Some(upload) = preview.upload.take()
        && let Some(mut material) = materials.get_mut(&scene.material)
    {
        let mut image = Image::new(
            Extent3d {
                width: upload.width,
                height: upload.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            upload.pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            ..default()
        });
        if let Some(old) = material.base_color_texture.replace(images.add(image)) {
            images.remove(&old);
        }
        material.alpha_mode = if upload.blend {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };
    }

    if let Some((before, after)) = preview.color_cube.take()
        && let Some(old) = scene
            .color_cube
            .replace((meshes.add(before), meshes.add(after)))
    {
        meshes.remove(&old.0);
        meshes.remove(&old.1);
    }

    if let Some(compiled) = preview.compiled.take() {
        let [gray, colored] = compiled_meshes(compiled);
        if let Some(old) = scene
            .compiled
            .replace([meshes.add(gray), meshes.add(colored)])
        {
            for handle in old {
                meshes.remove(&handle);
            }
        }
    }

    let (mesh, material, transform, visibility) = &mut *shape;
    // A model replaces the shape.
    let wanted_visibility = if preview.model.is_some() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    if **visibility != wanted_visibility {
        **visibility = wanted_visibility;
    }
    let current = if environment {
        preview.environment_shape()
    } else {
        preview.shape
    };
    let index = Shape::ALL.iter().position(|s| *s == current).unwrap_or(0);
    let compiled = scene
        .compiled
        .as_ref()
        .filter(|_| preview.compiled_shown && !environment);
    let (wanted_mesh, wanted_material) = match &scene.color_cube {
        _ if let Some(both) = compiled => (
            &both[usize::from(preview.meshlet_colors)],
            &scene.vertex_colors,
        ),
        Some((before, after)) if preview.colors => {
            let cube = if preview.colors_before { before } else { after };
            (cube, &scene.unlit)
        }
        _ if environment => (&scene.meshes[index], &scene.chrome),
        _ => (&scene.meshes[index], &scene.material),
    };
    if mesh.0 != *wanted_mesh {
        mesh.0 = wanted_mesh.clone();
    }
    if material.0 != *wanted_material {
        material.0 = wanted_material.clone();
    }
    // Compiled meshes are 3D files: they do not auto-rotate.
    if compiled.is_some() {
        transform.rotation = Quat::IDENTITY;
    } else if active && !preview.dragging {
        transform.rotate_y(SPIN * time.delta_secs());
    }
}

/// Gray and per-meshlet colored meshes of a compiled mesh, centered and scaled like a model;
/// see docs/features/models.md#compiled-mesh-preview.
fn compiled_meshes(preview: crate::amesh::Preview) -> [Mesh; 2] {
    let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for p in &preview.positions {
        (min, max) = (min.min(Vec3::from(*p)), max.max(Vec3::from(*p)));
    }
    let size = (max - min).max_element();
    let scale = if size > 0.0 { MODEL_SIZE / size } else { 1.0 };
    let center = (min + max) / 2.0;
    let positions: Vec<[f32; 3]> = preview
        .positions
        .iter()
        .map(|p| ((Vec3::from(*p) - center) * scale).to_array())
        .collect();
    let gray = Color::srgb(0.8, 0.8, 0.8).to_linear().to_f32_array();
    let colors = |by_meshlet: bool| -> Vec<[f32; 4]> {
        preview
            .meshlets
            .iter()
            .flat_map(|&m| {
                let color = if by_meshlet {
                    let hue = (m as f32 * 0.618_034).fract() * 360.0;
                    Color::hsv(hue, 0.65, 0.9).to_linear().to_f32_array()
                } else {
                    gray
                };
                [color; 3]
            })
            .collect()
    };
    [false, true].map(|by_meshlet| {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, preview.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors(by_meshlet))
    })
}

/// The model being shown, see docs/features/models.md#preview.
#[derive(Resource, Default)]
struct LoadedModel(Option<Model>);

struct Model {
    key: (PathBuf, u64),
    gltf: Handle<Gltf>,
    /// Waiting for a reload from disk to finish before spawning.
    reloading: bool,
    /// Holds the model; hidden until fitted.
    root: Entity,
    /// Centers and scales the scene, spawned as its child once the file has loaded.
    fit: Entity,
    /// The scene being shown, once spawned.
    scene: Option<Handle<WorldAsset>>,
    /// The scene's entities exist.
    ready: bool,
    /// The fit, once measured.
    fitted: Option<Transform>,
    /// The file's animations as one graph, and each one's node.
    graph: Option<(Handle<AnimationGraph>, Vec<AnimationNodeIndex>)>,
    /// The animation playing; `None` is the rest pose.
    playing: Option<usize>,
}

fn spawn_model_entities(commands: &mut Commands) -> (Entity, Entity) {
    let root = commands
        .spawn((Transform::default(), Visibility::Hidden))
        .id();
    let fit = commands
        .spawn((Transform::default(), Visibility::default(), ChildOf(root)))
        .id();
    (root, fit)
}

/// Spawns `scene` under `fit`, marking the model ready once its entities exist.
fn spawn_scene(commands: &mut Commands, fit: Entity, scene: Handle<WorldAsset>) {
    commands.entity(fit).insert(WorldAssetRoot(scene)).observe(
        |ready: On<WorldInstanceReady>, mut loaded: ResMut<LoadedModel>| {
            if let Some(model) = &mut loaded.0
                && model.fit == ready.event().entity
            {
                model.ready = true;
            }
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn sync_model(
    mut commands: Commands,
    mut preview: ResMut<Preview3d>,
    mut loaded: ResMut<LoadedModel>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut events: MessageReader<AssetEvent<Gltf>>,
    mut roots: Query<&mut Visibility>,
    (children, mut players, mut graphs): (
        Query<&Children>,
        Query<&mut AnimationPlayer>,
        ResMut<Assets<AnimationGraph>>,
    ),
) {
    let wanted = preview.model.clone().filter(|_| preview.viewport.is_some());
    if loaded.0.as_ref().map(|m| &m.key) != wanted.as_ref() {
        let previous = loaded.0.take();
        preview.model_status = ModelStatus::Idle;
        if let Some(key) = wanted {
            let path = AssetPath::from(key.0.clone()).with_source(INPUT_SOURCE);
            // Taken while the previous handle still holds the asset, so a reload applies to it.
            let gltf = server.load(path.clone());
            let reloading = previous.as_ref().is_some_and(|p| p.key.0 == key.0);
            if reloading {
                server.reload(path);
            }
            let (root, fit) = spawn_model_entities(&mut commands);
            loaded.0 = Some(Model {
                key,
                gltf,
                reloading,
                root,
                fit,
                scene: None,
                ready: false,
                fitted: None,
                graph: None,
                playing: None,
            });
            preview.model_status = ModelStatus::Loading(Instant::now());
        }
        if let Some(previous) = previous {
            commands.entity(previous.root).despawn();
        }
    }
    let Some(model) = &mut loaded.0 else {
        events.clear();
        return;
    };

    // A finished reload: spawn the new contents from scratch, and fit them again.
    for event in events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } = event
            && *id == model.gltf.id()
            && (model.reloading || matches!(event, AssetEvent::Modified { .. }))
        {
            if model.scene.is_some() {
                commands.entity(model.root).despawn();
                (model.root, model.fit) = spawn_model_entities(&mut commands);
                (model.scene, model.ready, model.fitted) = (None, false, None);
                (model.graph, model.playing) = (None, None);
                if !matches!(preview.model_status, ModelStatus::Loading(_)) {
                    preview.model_status = ModelStatus::Loading(Instant::now());
                }
            }
            model.reloading = false;
        }
    }

    if model.scene.is_none() {
        match server.load_state(&model.gltf) {
            LoadState::Failed(error) => {
                preview.model_status = ModelStatus::Failed(error.to_string());
            }
            LoadState::Loaded if !model.reloading => {
                if let Some(gltf) = gltfs.get(&model.gltf) {
                    match gltf
                        .default_scene
                        .clone()
                        .or_else(|| gltf.scenes.first().cloned())
                    {
                        Some(scene) => {
                            spawn_scene(&mut commands, model.fit, scene.clone());
                            model.scene = Some(scene);
                        }
                        None => {
                            preview.model_status =
                                ModelStatus::Failed("the file has no scene".into());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Animations; see docs/features/models.md#animations-tab.
    if model.ready
        && model.playing != preview.animation
        && let (Some(fitted), Some(scene)) = (model.fitted, model.scene.clone())
    {
        if model.playing.is_some() {
            // Back to the rest pose: spawn the scene again, fitted as before.
            commands.entity(model.fit).despawn();
            model.fit = commands
                .spawn((fitted, Visibility::default(), ChildOf(model.root)))
                .id();
            spawn_scene(&mut commands, model.fit, scene);
            (model.ready, model.playing) = (false, None);
        } else if let Some(index) = preview.animation
            && let Some(gltf) = gltfs.get(&model.gltf)
        {
            let (graph, nodes) = model.graph.get_or_insert_with(|| {
                let (graph, nodes) = AnimationGraph::from_clips(gltf.animations.iter().cloned());
                (graphs.add(graph), nodes)
            });
            if let Some(&node) = nodes.get(index) {
                for entity in children.iter_descendants(model.fit) {
                    if let Ok(mut player) = players.get_mut(entity) {
                        commands
                            .entity(entity)
                            .insert(AnimationGraphHandle(graph.clone()));
                        player.play(node).repeat();
                    }
                }
            }
            model.playing = Some(index);
        }
    }

    // Shown once fitted; models do not auto-rotate.
    if model.fitted.is_some()
        && let Ok(mut visibility) = roots.get_mut(model.root)
        && *visibility == Visibility::Hidden
    {
        *visibility = Visibility::Inherited;
        preview.model_status = ModelStatus::Ready;
    }
}

/// Centers and scales a model's scene once its entities exist, see docs/features/models.md#preview.
fn fit_model(
    mut loaded: ResMut<LoadedModel>,
    children: Query<&Children>,
    mesh_entities: Query<(&Mesh3d, &GlobalTransform, Option<&SkinnedMesh>)>,
    globals: Query<&GlobalTransform>,
    mut transforms: Query<&mut Transform>,
    meshes: Res<Assets<Mesh>>,
    bindposes: Res<Assets<SkinnedMeshInverseBindposes>>,
) {
    let Some(model) = &mut loaded.0 else {
        return;
    };
    if !model.ready || model.fitted.is_some() {
        return;
    }
    let Ok(fit_global) = globals.get(model.fit) else {
        return;
    };
    let to_fit = fit_global.affine().inverse();
    let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for entity in children.iter_descendants(model.fit) {
        let Ok((mesh, global, skin)) = mesh_entities.get(entity) else {
            continue;
        };
        let Some(positions) = meshes
            .get(&mesh.0)
            .and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION))
            .and_then(|a| a.as_float3())
        else {
            continue;
        };
        // A skinned mesh is placed by its joints: measure it through the first one.
        let skinned = skin.and_then(|skin| {
            let joint = globals.get(*skin.joints.first()?).ok()?;
            let bind = bindposes.get(&skin.inverse_bindposes)?.first()?;
            Some(joint.affine() * Affine3A::from_mat4(*bind))
        });
        let matrix = to_fit * skinned.unwrap_or_else(|| global.affine());
        for p in positions {
            let p = matrix.transform_point3(Vec3::from(*p));
            (min, max) = (min.min(p), max.max(p));
        }
    }
    let mut fitted = Transform::default();
    if min.cmple(max).all() {
        let size = (max - min).max_element();
        let scale = if size > 0.0 { MODEL_SIZE / size } else { 1.0 };
        fitted = Transform {
            translation: -(min + max) / 2.0 * scale,
            scale: Vec3::splat(scale),
            ..default()
        };
    }
    if let Ok(mut transform) = transforms.get_mut(model.fit) {
        *transform = fitted;
    }
    model.fitted = Some(fitted);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lut::Lut;

    #[test]
    fn cube_faces_wind_outward_and_stand_upright() {
        let mesh = cube(2.0);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("no indices");
        };
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| Vec3::from(positions[i as usize]));
            let center = (a + b + c) / 3.0;
            // Counter-clockwise seen from outside: the face normal points away from the center.
            assert!((b - a).cross(c - a).dot(center) > 0.0, "{triangle:?}");
        }
        // Side faces: the top edge (v = 0) is the upper one.
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("no uvs");
        };
        for face in 0..4 {
            for v in face * 4..face * 4 + 4 {
                assert_eq!(uvs[v][1] == 0.0, positions[v][1] > 0.0);
            }
        }
    }

    #[test]
    fn color_cube_maps_inputs_through_the_lut() {
        let mut text = String::from("LUT_3D_SIZE 2\n");
        // Inverting LUT.
        for i in 0..8 {
            text += &format!(
                "{} {} {}\n",
                1 - (i & 1),
                1 - ((i >> 1) & 1),
                1 - ((i >> 2) & 1)
            );
        }
        let lut = Lut::parse(&text).unwrap();
        let mesh = color_cube(lut.size, |rgb| lut.apply(rgb));
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("no colors");
        };
        assert_eq!(positions.len(), 6 * 4);
        for (p, c) in positions.iter().zip(colors) {
            // Input red at +X (x = 0.6) inverts to 0.
            let expected = if p[0] > 0.0 { 0.0 } else { 1.0 };
            assert_eq!(c[0], expected, "{p:?} {c:?}");
        }
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("no indices");
        };
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = triangle.map(|i| Vec3::from(positions[i as usize]));
            assert!((b - a).cross(c - a).dot((a + b + c) / 3.0) > 0.0);
        }
    }

    #[test]
    fn orbit_limits() {
        let mut orbit = Orbit::default();
        orbit.drag(Vec2::new(0.0, 10_000.0));
        assert!((orbit.pitch - 85f32.to_radians()).abs() < 1e-5);
        orbit.dolly(1000.0);
        assert_eq!(orbit.distance, 1.5);
        orbit.dolly(0.001);
        assert_eq!(orbit.distance, 10.0);
    }

    #[test]
    fn camera_looks_at_center_from_distance() {
        let orbit = Orbit::default();
        let t = orbit.transform();
        assert!((t.translation.length() - 3.5).abs() < 1e-4);
        // Pitched up above the horizon, looking down at the origin.
        assert!(t.translation.y > 0.0);
        let forward = t.forward();
        assert!((forward.dot(-t.translation.normalize()) - 1.0).abs() < 1e-4);
    }
}
