//! The canvas' 3D mode: the selected image on a rotating, lit shape, rendered
//! behind the egui overlay. See docs/features/canvas.md#3d.

use bevy::asset::RenderAssetUsages;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{GeneratedEnvironmentMapLight, Skybox};
use bevy::mesh::{CylinderMeshBuilder, Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};

use crate::lut::Lut;
use crate::theme::BG_CENTER;

pub struct Preview3dPlugin;

impl Plugin for Preview3dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Preview3d>()
            .insert_resource(GlobalAmbientLight {
                brightness: AMBIENT_BRIGHTNESS,
                ..default()
            })
            .add_systems(Startup, setup)
            .add_systems(PostUpdate, sync);
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
    pub orbit: Orbit,
    /// Being orbited: auto-rotation pauses.
    pub dragging: bool,
    /// A new texture to show, taken by [`sync`].
    pub upload: Option<Upload>,
    /// Show the environment map (skybox + lighting) instead of the textured shape.
    pub environment: bool,
    /// Show the color cube (unlit, vertex colors) instead of the shape.
    pub colors: bool,
    /// A new color cube mesh, taken by [`sync`].
    pub color_cube: Option<Mesh>,
    /// A new environment map, taken by [`sync`].
    pub cubemap: Option<CubemapUpload>,
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
    color_cube: Option<Handle<Mesh>>,
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

/// The RGB cube transformed by `lut`: red along +X, green +Y, blue +Z, each face an
/// N × N grid colored with the LUT output; see docs/features/lut.md#cube.
pub fn color_cube(lut: &Lut) -> Mesh {
    const SIZE: f32 = 1.2;
    let n = lut.size;
    let (mut positions, mut colors, mut indices) = (vec![], vec![], vec![]);
    for (normal, right, up) in CUBE_FACES {
        let base = positions.len() as u32;
        for j in 0..n {
            for i in 0..n {
                let (x, y) = (i as f32 / (n - 1) as f32, j as f32 / (n - 1) as f32);
                let p = (normal + right * (2.0 * x - 1.0) + up * (2.0 * y - 1.0)) * 0.5;
                let input = (p + 0.5).to_array();
                let [r, g, b] = lut.apply(input);
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
    mut shape: Single<ShapeQuery, (With<PreviewShape>, Without<PreviewCamera>)>,
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

    if let Some(cube) = preview.color_cube.take()
        && let Some(old) = scene.color_cube.replace(meshes.add(cube))
    {
        meshes.remove(&old);
    }

    let (mesh, material, transform) = &mut *shape;
    let index = Shape::ALL
        .iter()
        .position(|s| *s == preview.shape)
        .unwrap_or(0);
    let (wanted_mesh, wanted_material) = match &scene.color_cube {
        Some(cube) if preview.colors => (cube, &scene.unlit),
        _ if environment => (&scene.meshes[index], &scene.chrome),
        _ => (&scene.meshes[index], &scene.material),
    };
    if mesh.0 != *wanted_mesh {
        mesh.0 = wanted_mesh.clone();
    }
    if material.0 != *wanted_material {
        material.0 = wanted_material.clone();
    }
    if active && !preview.dragging {
        transform.rotate_y(SPIN * time.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mesh = color_cube(&Lut::parse(&text).unwrap());
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
