//! The canvas' 3D mode: the selected image on a rotating, lit shape, rendered
//! behind the egui overlay. See docs/features/canvas.md#3d.

use std::path::PathBuf;
use std::time::Instant;

use bevy::asset::{AssetPath, LoadState, RenderAssetUsages};
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::core_pipeline::fullscreen_material::{FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::gltf::Gltf;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::{GeneratedEnvironmentMapLight, Skybox};
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{CylinderMeshBuilder, Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::{
    Extent3d, ShaderType, TextureDimension, TextureFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use bevy::shader::ShaderRef;

use bevy::transform::TransformSystems;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};

use crate::theme::BG_CENTER;
use crate::vision::Vision;

/// Asset source for the project's input directory, registered at startup; models load from it.
pub const INPUT_SOURCE: &str = "input";
/// Vertical field of view of the preview camera.
pub const FOV_DEGREES: f32 = 45.0;
/// Longest side of a fitted model's bounding box, in units.
pub const MODEL_SIZE: f32 = 1.5;

pub struct Preview3dPlugin;

impl Plugin for Preview3dPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "vision.wgsl");
        app.add_plugins(FullscreenMaterialPlugin::<VisionFilter>::default())
            .init_resource::<Preview3d>()
            .insert_resource(GlobalAmbientLight {
                brightness: AMBIENT_BRIGHTNESS,
                ..default()
            })
            .init_resource::<LoadedModel>()
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                (
                    (
                        sync_compiled,
                        sync,
                        sync_material,
                        sync_model,
                        sync_playback,
                        sync_vision,
                    )
                        .chain()
                        .before(TransformSystems::Propagate),
                    (sync_attachments, sync_region_view, sync_grid)
                        .after(sync_model)
                        .before(TransformSystems::Propagate),
                    (fit_model, report_frames, cast_surface, report_regions)
                        .chain()
                        .after(TransformSystems::Propagate),
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
        // A view from straight above or below drags back within the limit.
        let limit = limit.max(self.pitch.abs());
        self.pitch = (self.pitch + delta.y * step).clamp(-limit, limit);
    }

    /// Looks along world axis `axis` (0 X, 1 Y, 2 Z) toward the center from its `positive` or
    /// negative side; see docs/features/canvas.md#orientation-gizmo.
    pub fn look_from(&mut self, axis: usize, positive: bool) {
        let side = if positive { 1.0 } else { -1.0 };
        let (yaw, pitch) = match axis {
            0 => (90f32 * side, 0.0),
            1 => (0.0, 90.0 * side),
            _ => (if positive { 0.0 } else { 180.0 }, 0.0),
        };
        (self.yaw, self.pitch) = (yaw.to_radians(), pitch.to_radians());
    }

    /// Divides the distance by `factor` (> 1 moves closer).
    pub fn dolly(&mut self, factor: f32) {
        self.distance =
            (self.distance / factor).clamp(*Self::DISTANCE.start(), *Self::DISTANCE.end());
    }

    pub fn transform(&self) -> Transform {
        // Turned rather than aimed, so straight above or below has an up direction too.
        let rotation = Quat::from_euler(EulerRot::YXZ, self.yaw, -self.pitch, 0.0);
        Transform::from_translation(rotation * Vec3::new(0.0, 0.0, self.distance))
            .with_rotation(rotation)
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
    /// Hold the playing animation where it is; see docs/features/animation-events.md#timeline.
    pub animation_paused: bool,
    /// A time (seconds) to move the playing animation to, taken by [`sync_playback`].
    pub animation_seek: Option<f32>,
    /// The playing animation's time, written by [`sync_playback`]; `None` while none plays.
    pub animation_time: Option<f32>,
    /// A compiled mesh to show instead of the shape, taken by [`sync_compiled`].
    pub compiled: Option<CompiledUpload>,
    /// New positions and normals for the last compiled mesh uploaded (a posed skin), per
    /// corner of its level, taken by [`sync_compiled`]; see
    /// docs/features/models.md#compiled-animation-preview.
    pub compiled_pose: Option<Pose>,
    /// Show the last compiled mesh uploaded.
    pub compiled_shown: bool,
    pub display: MeshDisplay,
    /// Which [`MeshOverlay`]s are drawn, by index in [`MeshOverlay::ALL`].
    pub overlays: [bool; 6],
    /// The color vision the view simulates; `None` is normal vision.
    pub vision: Option<Vision>,
    /// An `.amat` being edited: its values this frame (`None` when no material is shown); see
    /// docs/features/materials.md#3d-preview.
    pub material: Option<crate::material::Material>,
    /// Its textures, sent once per inspection.
    pub material_textures: Option<MaterialTextures>,
    /// The shape materials are shown on; `None` is a sphere.
    pub material_shape: Option<Shape>,
    /// Where the shown model's locators hang, once it is fitted; written by [`report_frames`].
    pub frames: Option<ModelFrames>,
    /// A ray (origin, unit direction) to cast against the shown model, taken by
    /// [`cast_surface`], which answers in `surface_hit`.
    pub surface_ray: Option<SurfaceRay>,
    /// The answer to the last `surface_ray`, for the UI to take: `Some(None)` is a miss.
    pub surface_hit: Option<Option<SurfaceHit>>,
    /// Models shown at the shown model's locators, written by the UI every frame.
    pub attachments: Vec<Attachment>,
    /// The visible regions' triangles, written by the UI every frame; see
    /// docs/features/regions.md#3d-view.
    pub region_wants: Vec<RegionWant>,
    /// Draw the shown model's ground grid; see docs/features/models.md#ground-grid.
    pub grid: bool,
    /// Draw the model in its regions' colors (from `region_wants`) instead of its materials.
    pub region_view: bool,
    /// Draw the model in a color set's colors instead of its materials (or regions); see
    /// docs/features/vertex-colors.md#3d-view.
    pub color_view: Option<std::sync::Arc<ColorView>>,
    /// The region whose triangles to report in `region_triangles`.
    pub region_outline: Option<usize>,
    /// The outlined region's triangles as posed this frame: region and corners; written by
    /// [`report_regions`].
    pub region_triangles: Vec<(usize, [Vec3; 3])>,
}

/// A ray to cast against the shown model.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceRay {
    pub origin: Vec3,
    /// Unit length.
    pub dir: Vec3,
    /// Triangles whose center lies this close to the hit are reported too (painting).
    pub brush: f32,
}

/// A glTF triangle: mesh, primitive and index in the primitive.
pub type TriangleId = (u32, u32, u32);

/// Triangles of one glTF primitive wanted for a region: all of them, or some.
#[derive(Clone, Debug)]
pub struct RegionWant {
    pub region: usize,
    /// The region's color, sRGB.
    pub color: [u8; 3],
    pub mesh: u32,
    pub primitive: u32,
    pub triangles: Option<Vec<u32>>,
}

/// A glTF primitive (mesh, primitive) and a color per triangle corner.
pub type Corners = ((u32, u32), Vec<[u8; 4]>);

/// A color set to show on the model, one color per triangle corner.
#[derive(Clone, Debug, Default)]
pub struct ColorView {
    /// Different whenever `corners` are.
    pub generation: u64,
    /// Each glTF primitive's linear RGBA8 colors, by mesh and primitive, in the glTF's
    /// triangle order.
    pub corners: Vec<Corners>,
}

/// Where a ray hit the shown model; see docs/features/locators.md#place-on-surface.
#[derive(Clone, Debug)]
pub struct SurfaceHit {
    pub point: Vec3,
    /// The triangle's unit normal, facing the ray's origin.
    pub normal: Vec3,
    /// The named joint weighing most on the triangle, for skinned meshes.
    pub joint: Option<String>,
    /// The triangles in the ray's brush facing the ray, the hit one first.
    pub brush: Vec<TriangleId>,
}

/// A model shown at a locator; see docs/features/locators.md#attachment-preview.
#[derive(Clone, Debug, PartialEq)]
pub struct Attachment {
    /// Input-relative glTF path.
    pub path: PathBuf,
    /// The joint the locator hangs on, by name; `None` is model space.
    pub joint: Option<String>,
    /// The locator's transform in that space.
    pub transform: Transform,
    /// The attached model's own locator put on this one: its joint in the attached model (by
    /// name; `None` is its model space) and its transform there. `None` holds it by its origin.
    pub held: Option<(Option<String>, Transform)>,
}

/// World transforms of a shown model's spaces, as posed this frame; see
/// docs/features/locators.md#3d-view.
#[derive(Clone, Debug)]
pub struct ModelFrames {
    /// Model space: the scene's root, as fitted.
    pub model: Affine3A,
    /// The first skin's joints, by name, in skin order.
    pub joints: Vec<(String, Affine3A)>,
}

impl ModelFrames {
    /// The world transform of `joint`'s space (the first with that name), or the model's.
    pub fn parent(&self, joint: Option<&str>) -> Option<Affine3A> {
        match joint {
            None => Some(self.model),
            Some(name) => self
                .joints
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, frame)| *frame),
        }
    }
}

/// The color vision simulation pass on the preview camera; see
/// docs/features/canvas.md#color-vision.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default, PartialEq)]
#[extract_app(RenderApp)]
struct VisionFilter {
    r: Vec4,
    g: Vec4,
    b: Vec4,
}

impl FullscreenMaterial for VisionFilter {
    fn fragment_shader() -> ShaderRef {
        "embedded://assetc/vision.wgsl".into()
    }
}

/// Adds, changes or removes the camera's [`VisionFilter`] to match [`Preview3d::vision`].
fn sync_vision(
    mut commands: Commands,
    preview: Res<Preview3d>,
    camera: Single<(Entity, Option<&VisionFilter>), With<PreviewCamera>>,
) {
    let (entity, current) = *camera;
    let wanted = preview.vision.map(|vision| {
        let [r, g, b] = vision.matrix().map(|row| Vec3::from(row).extend(0.0));
        VisionFilter { r, g, b }
    });
    if current.copied() != wanted {
        match wanted {
            Some(filter) => commands.entity(entity).insert(filter),
            None => commands.entity(entity).remove::<VisionFilter>(),
        };
    }
}

/// A compiled mesh and its material slots, see docs/features/models.md#compiled-mesh-preview.
pub struct CompiledUpload {
    /// The level of detail to show.
    pub level: crate::amesh::Level,
    /// The color set to show, by index in the level's; see
    /// docs/features/vertex-colors.md#compiled-meshes.
    pub color_set: Option<usize>,
    /// Bounds of the full mesh (min, max), so every level is fitted alike.
    pub bounds: [Vec3; 2],
    pub materials: Vec<Option<crate::inspect::MaterialPreview>>,
    /// The file's bounds (`BOUNDS`, each submesh's, each meshlet's) and each meshlet's box.
    pub mesh_bounds: crate::amesh::Bounds,
    pub submesh_bounds: Vec<crate::amesh::Bounds>,
    pub meshlet_bounds: Vec<crate::amesh::MeshletBounds>,
    pub meshlet_boxes: Vec<[[f32; 3]; 2]>,
}

/// Positions and normals per corner of a compiled mesh's level.
pub type Pose = (Vec<[f32; 3]>, Vec<[f32; 3]>);

/// Bounds drawn over a compiled mesh; see docs/features/models.md#compiled-mesh-preview.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeshOverlay {
    BoundingBox,
    BoundingSphere,
    SubmeshBoxes,
    MeshletBoxes,
    MeshletSpheres,
    MeshletCones,
}

impl MeshOverlay {
    pub const ALL: [Self; 6] = [
        Self::BoundingBox,
        Self::BoundingSphere,
        Self::SubmeshBoxes,
        Self::MeshletBoxes,
        Self::MeshletSpheres,
        Self::MeshletCones,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::BoundingBox => "Bounding box",
            Self::BoundingSphere => "Bounding sphere",
            Self::SubmeshBoxes => "Submesh boxes",
            Self::MeshletBoxes => "Meshlet boxes",
            Self::MeshletSpheres => "Meshlet spheres",
            Self::MeshletCones => "Meshlet cones",
        }
    }

    /// Drawn from the meshlets, which only LOD 0 has.
    pub fn of_meshlets(self) -> bool {
        matches!(
            self,
            Self::MeshletBoxes | Self::MeshletSpheres | Self::MeshletCones
        )
    }
}

/// How a compiled mesh is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum MeshDisplay {
    #[default]
    Material,
    Wireframe,
    Shaded,
    Meshlets,
    MaterialId,
    Submeshes,
}

impl MeshDisplay {
    pub const ALL: [Self; 6] = [
        Self::Material,
        Self::Shaded,
        Self::Wireframe,
        Self::Meshlets,
        Self::MaterialId,
        Self::Submeshes,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Material => "Material",
            Self::Wireframe => "Wireframe",
            Self::Shaded => "Shaded",
            Self::Meshlets => "Meshlets",
            Self::MaterialId => "Material ID",
            Self::Submeshes => "Submeshes",
        }
    }

    /// Index of this mode's vertex-colored mesh, for the modes that use one.
    fn colored(self) -> Option<usize> {
        match self {
            Self::Shaded => Some(0),
            Self::Meshlets => Some(1),
            Self::MaterialId => Some(2),
            Self::Submeshes => Some(3),
            Self::Material | Self::Wireframe => None,
        }
    }
}

/// An entity of the compiled mesh: a material part, or a [`CompiledOverlay`].
#[derive(Component)]
struct CompiledPart;

/// The lines of one [`MeshOverlay`], by index in [`MeshOverlay::ALL`].
#[derive(Component)]
struct CompiledOverlay(usize);

/// A compiled mesh on the GPU: vertex-colored meshes, the wireframe, and one entity per
/// material slot for the Material mode, with every asset they own.
struct CompiledScene {
    colored: [Handle<Mesh>; 4],
    wire: Handle<Mesh>,
    /// The fit (center and scale), and each material part's mesh and corners, for poses.
    center: Vec3,
    scale: f32,
    part_meshes: Vec<Handle<Mesh>>,
    part_corners: Vec<Vec<usize>>,
    parts: Vec<Entity>,
    overlays: Vec<Entity>,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
    images: Vec<Handle<Image>>,
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

    pub fn material_shape(&self) -> Shape {
        self.material_shape.unwrap_or(Shape::Sphere)
    }
}

/// An edited material's textures, as the build would use them: base color and emissive sRGB,
/// the rest linear; `depth` is `1 - height`.
#[derive(Clone, Default)]
pub struct MaterialTextures {
    pub base_color: Option<crate::texture::Image>,
    pub normal: Option<crate::texture::Image>,
    pub orm: Option<crate::texture::Image>,
    pub emissive: Option<crate::texture::Image>,
    pub depth: Option<crate::texture::Image>,
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
    compiled: Option<CompiledScene>,
    /// Lit, showing vertex colors.
    vertex_colors: Handle<StandardMaterial>,
    /// Unlit lines for wireframes.
    wire: Handle<StandardMaterial>,
    cubemap: Option<Handle<Image>>,
    /// The `.amat` being edited, and the textures it owns.
    edited: Handle<StandardMaterial>,
    edited_images: Vec<Handle<Image>>,
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
    // Tangents, for a material's normal and depth maps.
    let mut add = |mut mesh: Mesh| {
        let _ = mesh.generate_tangents();
        meshes.add(mesh)
    };
    let meshes = [
        add(cube(1.2)),
        // Bevy's UV sphere has its poles on Z; stand it up so rows of the image run around.
        add(Sphere::new(0.75)
            .mesh()
            .uv(48, 24)
            .rotated_by(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))),
        add(CylinderMeshBuilder::new(0.6, 1.4, 48).build()),
        add(Torus::new(0.35, 0.85).mesh().build()),
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
    let [tr, tg, tb, _] = crate::theme::TEXT.to_array();
    let wire = materials.add(StandardMaterial {
        base_color: Color::srgb_u8(tr, tg, tb),
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
                fov: FOV_DEGREES.to_radians(),
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
    let edited = materials.add(StandardMaterial::default());
    commands.insert_resource(Scene {
        edited,
        edited_images: Vec::new(),
        meshes,
        material,
        chrome,
        unlit,
        color_cube: None,
        compiled: None,
        vertex_colors,
        wire,
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

    let compiled = scene
        .compiled
        .as_ref()
        .filter(|_| preview.compiled_shown && !environment);
    let (mesh, material, transform, visibility) = &mut *shape;
    // A model replaces the shape, as do a compiled mesh's material parts.
    let parts_shown = compiled.is_some() && preview.display == MeshDisplay::Material;
    let wanted_visibility = if preview.model.is_some() || parts_shown {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    if **visibility != wanted_visibility {
        **visibility = wanted_visibility;
    }
    let current = if preview.material.is_some() {
        preview.material_shape()
    } else if environment {
        preview.environment_shape()
    } else {
        preview.shape
    };
    let index = Shape::ALL.iter().position(|s| *s == current).unwrap_or(0);
    let (wanted_mesh, wanted_material) = match &scene.color_cube {
        _ if let Some(c) = compiled
            && let Some(i) = preview.display.colored() =>
        {
            (&c.colored[i], &scene.vertex_colors)
        }
        _ if let Some(c) = compiled => (&c.wire, &scene.wire),
        Some((before, after)) if preview.colors => {
            let cube = if preview.colors_before { before } else { after };
            (cube, &scene.unlit)
        }
        _ if preview.material.is_some() => (&scene.meshes[index], &scene.edited),
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

/// Gives the edited material its textures and values; see
/// docs/features/materials.md#3d-preview.
fn sync_material(
    mut preview: ResMut<Preview3d>,
    mut scene: ResMut<Scene>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: Local<Option<crate::material::Material>>,
) {
    let edited = scene.edited.clone();
    if let Some(textures) = preview.material_textures.take()
        && let Some(mut m) = materials.get_mut(&edited)
    {
        for old in scene.edited_images.drain(..) {
            images.remove(&old);
        }
        let mut add = |image: Option<crate::texture::Image>, srgb: bool| {
            image.map(|i| {
                let handle = images.add(bevy_image(&i, srgb));
                scene.edited_images.push(handle.clone());
                handle
            })
        };
        let orm = add(textures.orm, false);
        m.base_color_texture = add(textures.base_color, true);
        m.normal_map_texture = add(textures.normal, false);
        m.metallic_roughness_texture = orm.clone();
        m.occlusion_texture = orm;
        m.emissive_texture = add(textures.emissive, true);
        m.depth_map = add(textures.depth, false);
    }
    let Some(a) = preview.material.clone() else {
        return;
    };
    if shown.as_ref() == Some(&a) {
        return;
    }
    if let Some(mut m) = materials.get_mut(&edited) {
        let [r, g, b, alpha] = a.base_color;
        let [er, eg, eb] = a.emissive.map(|c| c * a.emissive_strength);
        m.base_color = Color::linear_rgba(r, g, b, alpha);
        m.metallic = a.metallic;
        m.perceptual_roughness = a.roughness;
        m.emissive = LinearRgba::rgb(er, eg, eb);
        m.parallax_depth_scale = a.height_scale;
        m.alpha_mode = match a.alpha_mode {
            crate::material::AlphaMode::Opaque => AlphaMode::Opaque,
            crate::material::AlphaMode::Mask => AlphaMode::Mask(a.alpha_cutoff),
            crate::material::AlphaMode::Blend => AlphaMode::Blend,
        };
        m.double_sided = a.double_sided;
        m.cull_mode = (!a.double_sided).then_some(bevy::render::render_resource::Face::Back);
    }
    *shown = Some(a);
}

/// Uploads a compiled mesh and shows its material parts in the Material mode; see
/// docs/features/models.md#compiled-mesh-preview.
fn sync_compiled(
    mut commands: Commands,
    mut preview: ResMut<Preview3d>,
    mut scene: ResMut<Scene>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut parts: Query<(&mut Visibility, Option<&CompiledOverlay>), With<CompiledPart>>,
) {
    if let Some(upload) = preview.compiled.take() {
        if let Some(old) = scene.compiled.take() {
            for entity in old.parts.into_iter().chain(old.overlays) {
                commands.entity(entity).despawn();
            }
            for handle in old.meshes {
                meshes.remove(&handle);
            }
            for handle in old.materials {
                materials.remove(&handle);
            }
            for handle in old.images {
                images.remove(&handle);
            }
        }
        let unlit = scene.unlit.clone();
        scene.compiled = Some(build_compiled(
            upload,
            &unlit,
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut images,
        ));
    }
    // A pose: the same corners moved, nothing rebuilt.
    if let (Some((positions, normals)), Some(compiled)) =
        (preview.compiled_pose.take(), scene.compiled.as_ref())
    {
        let placed: Vec<[f32; 3]> = positions
            .iter()
            .map(|p| ((Vec3::from(*p) - compiled.center) * compiled.scale).to_array())
            .collect();
        for handle in &compiled.colored {
            if let Some(mut mesh) = meshes.get_mut(handle) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, placed.clone());
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals.clone());
            }
        }
        if let Some(mut mesh) = meshes.get_mut(&compiled.wire) {
            let lines: Vec<[f32; 3]> = placed
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|[a, b, c]| [*a, *b, *b, *c, *c, *a])
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, lines);
        }
        for (handle, corners) in compiled.part_meshes.iter().zip(&compiled.part_corners) {
            if let Some(mut mesh) = meshes.get_mut(handle) {
                let pick = |values: &[[f32; 3]]| -> Vec<[f32; 3]> {
                    corners
                        .iter()
                        .filter_map(|&c| values.get(c).copied())
                        .collect()
                };
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pick(&placed));
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, pick(&normals));
            }
        }
    }
    let visible = |shown: bool| {
        if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    let compiled = preview.viewport.is_some() && preview.compiled_shown && !preview.environment;
    for (mut visibility, overlay) in &mut parts {
        let wanted = visible(
            compiled
                && match overlay {
                    Some(overlay) => preview.overlays[overlay.0],
                    None => preview.display == MeshDisplay::Material,
                },
        );
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// Lines of a box's 12 edges.
fn box_lines(lines: &mut Vec<[f32; 3]>, min: Vec3, max: Vec3) {
    let corner = |i: usize| {
        Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        )
        .to_array()
    };
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                lines.extend([corner(i), corner(i | bit)]);
            }
        }
    }
}

/// Lines of a circle around `center` in the plane of the unit vectors `u`, `v`.
fn circle_lines(
    lines: &mut Vec<[f32; 3]>,
    center: Vec3,
    u: Vec3,
    v: Vec3,
    radius: f32,
    segments: usize,
) {
    let point = |k: usize| {
        let a = k as f32 / segments as f32 * std::f32::consts::TAU;
        (center + (u * a.cos() + v * a.sin()) * radius).to_array()
    };
    for k in 0..segments {
        lines.extend([point(k), point(k + 1)]);
    }
}

/// Three great circles, in the XY, YZ and XZ planes.
fn sphere_lines(lines: &mut Vec<[f32; 3]>, center: Vec3, radius: f32) {
    for (u, v) in [(Vec3::X, Vec3::Y), (Vec3::Y, Vec3::Z), (Vec3::X, Vec3::Z)] {
        circle_lines(lines, center, u, v, radius, 48);
    }
}

/// A normal cone: apex at `apex`, around `axis`, `half_angle` wide, `slant` long.
fn cone_lines(lines: &mut Vec<[f32; 3]>, apex: Vec3, axis: Vec3, half_angle: f32, slant: f32) {
    let (u, v) = axis.any_orthonormal_pair();
    let base = apex + axis * slant * half_angle.cos();
    let radius = slant * half_angle.sin();
    lines.extend([apex.to_array(), base.to_array()]);
    circle_lines(lines, base, u, v, radius, 32);
    for dir in [u, v, -u, -v] {
        lines.extend([apex.to_array(), (base + dir * radius).to_array()]);
    }
}

/// Color of index `i` in the ID modes: golden-ratio hues.
fn id_color(i: u32) -> [f32; 4] {
    let hue = (i as f32 * 0.618_034).fract() * 360.0;
    Color::hsv(hue, 0.65, 0.9).to_linear().to_f32_array()
}

fn bevy_image(image: &crate::texture::Image, srgb: bool) -> Image {
    let format = if srgb {
        TextureFormat::Rgba8UnormSrgb
    } else {
        TextureFormat::Rgba8Unorm
    };
    let mut out = Image::new(
        Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image.pixels.clone(),
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    out.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    out
}

fn build_compiled(
    upload: CompiledUpload,
    unlit: &Handle<StandardMaterial>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> CompiledScene {
    let p = &upload.level;
    // Centered and scaled like a model.
    let [min, max] = upload.bounds;
    let size = (max - min).max_element();
    let scale = if size > 0.0 { MODEL_SIZE / size } else { 1.0 };
    let center = (min + max) / 2.0;
    let positions: Vec<[f32; 3]> = p
        .positions
        .iter()
        .map(|v| ((Vec3::from(*v) - center) * scale).to_array())
        .collect();
    let gray = Color::srgb(0.8, 0.8, 0.8).to_linear().to_f32_array();

    // Shaded, Meshlets, Material ID, Submeshes.
    let per_triangle = |color: &dyn Fn(usize) -> [f32; 4]| -> Vec<[f32; 4]> {
        (0..p.triangles()).flat_map(|t| [color(t); 3]).collect()
    };
    let chosen = upload.color_set.and_then(|s| p.colors.get(s));
    let colors = [
        // Shaded: the chosen color set, or gray.
        chosen.cloned().unwrap_or_else(|| per_triangle(&|_| gray)),
        // Reduced levels have no meshlets.
        per_triangle(&|t| p.meshlets.get(t).map_or(gray, |&m| id_color(m))),
        per_triangle(&|t| p.material_slots[t].map_or(gray, id_color)),
        per_triangle(&|t| id_color(p.submeshes[t])),
    ];
    let mut owned_meshes = Vec::new();
    let colored = colors.map(|colors| {
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, p.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        let handle = meshes.add(mesh);
        owned_meshes.push(handle.clone());
        handle
    });

    // Every triangle edge.
    let lines: Vec<[f32; 3]> = positions
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|[a, b, c]| [*a, *b, *b, *c, *c, *a])
        .collect();
    let wire = meshes.add(
        Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, lines),
    );
    owned_meshes.push(wire.clone());

    // Material mode: one part per material slot (and one for triangles without a material).
    let mut slots: Vec<Option<u32>> = p.material_slots.clone();
    slots.sort();
    slots.dedup();
    let (mut parts, mut owned_materials, mut owned_images) = (Vec::new(), Vec::new(), Vec::new());
    let (mut part_meshes, mut part_corners) = (Vec::new(), Vec::new());
    for slot in slots {
        let corners: Vec<usize> = (0..p.material_slots.len())
            .filter(|&t| p.material_slots[t] == slot)
            .flat_map(|t| [t * 3, t * 3 + 1, t * 3 + 2])
            .collect();
        let pick = |values: &[[f32; 3]]| corners.iter().map(|&c| values[c]).collect::<Vec<_>>();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pick(&positions))
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, pick(&p.normals))
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_UV_0,
            corners.iter().map(|&c| p.uvs[c]).collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_TANGENT,
            corners.iter().map(|&c| p.tangents[c]).collect::<Vec<_>>(),
        );
        // The chosen color set multiplies the base color; see
        // docs/features/vertex-colors.md#compiled-meshes.
        if let Some(colors) = chosen {
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_COLOR,
                corners.iter().map(|&c| colors[c]).collect::<Vec<_>>(),
            );
        }
        let found = slot.and_then(|s| upload.materials.get(s as usize).cloned().flatten());
        let mut texture = |image: &Option<crate::texture::Image>, srgb: bool| {
            image.as_ref().map(|i| {
                let handle = images.add(bevy_image(i, srgb));
                owned_images.push(handle.clone());
                handle
            })
        };
        let material = match (slot, &found) {
            (None, _) => StandardMaterial {
                base_color: Color::srgb(0.8, 0.8, 0.8),
                perceptual_roughness: 0.8,
                ..default()
            },
            (Some(_), None) => StandardMaterial {
                perceptual_roughness: 1.0,
                ..default()
            },
            (Some(_), Some(m)) => {
                let a = &m.material;
                let orm = texture(&m.orm, false);
                let [r, g, b, alpha] = a.base_color;
                let [er, eg, eb] = a.emissive.map(|c| c * a.emissive_strength);
                StandardMaterial {
                    base_color: Color::linear_rgba(r, g, b, alpha),
                    base_color_texture: texture(&m.base_color, true),
                    metallic: a.metallic,
                    perceptual_roughness: a.roughness,
                    metallic_roughness_texture: orm.clone(),
                    occlusion_texture: orm,
                    normal_map_texture: texture(&m.normal, false),
                    emissive: LinearRgba::rgb(er, eg, eb),
                    emissive_texture: texture(&m.emissive, true),
                    alpha_mode: match a.alpha_mode {
                        crate::material::AlphaMode::Opaque => AlphaMode::Opaque,
                        crate::material::AlphaMode::Mask => AlphaMode::Mask(a.alpha_cutoff),
                        crate::material::AlphaMode::Blend => AlphaMode::Blend,
                    },
                    double_sided: a.double_sided,
                    cull_mode: (!a.double_sided)
                        .then_some(bevy::render::render_resource::Face::Back),
                    ..default()
                }
            }
        };
        let mesh = meshes.add(mesh);
        part_meshes.push(mesh.clone());
        part_corners.push(corners);
        let material = materials.add(material);
        owned_meshes.push(mesh.clone());
        owned_materials.push(material.clone());
        parts.push(
            commands
                .spawn((
                    CompiledPart,
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id(),
        );
    }

    // Bounds, as vertex-colored lines placed like the mesh.
    let place = |p: [f32; 3]| (Vec3::from(p) - center) * scale;
    let [ar, ag, ab, _] = crate::theme::ACCENT.to_array();
    let accent = Color::srgb_u8(ar, ag, ab).to_linear().to_f32_array();
    let mut overlays = Vec::new();
    for (i, overlay) in MeshOverlay::ALL.into_iter().enumerate() {
        let (mut lines, mut colors) = (Vec::new(), Vec::new());
        let mut add =
            |lines: &mut Vec<[f32; 3]>, color: [f32; 4], draw: &dyn Fn(&mut Vec<[f32; 3]>)| {
                let start = lines.len();
                draw(lines);
                colors.resize(colors.len() + lines.len() - start, color);
            };
        let b = &upload.mesh_bounds;
        match overlay {
            MeshOverlay::BoundingBox => add(&mut lines, accent, &|l| {
                box_lines(l, place(b.min), place(b.max))
            }),
            MeshOverlay::BoundingSphere => add(&mut lines, accent, &|l| {
                sphere_lines(l, place(b.center), b.radius * scale)
            }),
            MeshOverlay::SubmeshBoxes => {
                for (s, b) in upload.submesh_bounds.iter().enumerate() {
                    add(&mut lines, id_color(s as u32), &|l| {
                        box_lines(l, place(b.min), place(b.max))
                    });
                }
            }
            MeshOverlay::MeshletBoxes => {
                for (m, [min, max]) in upload.meshlet_boxes.iter().enumerate() {
                    if min[0] <= max[0] {
                        add(&mut lines, id_color(m as u32), &|l| {
                            box_lines(l, place(*min), place(*max))
                        });
                    }
                }
            }
            MeshOverlay::MeshletSpheres => {
                for (m, b) in upload.meshlet_bounds.iter().enumerate() {
                    add(&mut lines, id_color(m as u32), &|l| {
                        sphere_lines(l, place(b.center), b.radius * scale)
                    });
                }
            }
            MeshOverlay::MeshletCones => {
                for (m, b) in upload.meshlet_bounds.iter().enumerate() {
                    let axis = Vec3::from(b.cone_axis).normalize_or_zero();
                    if b.cone_cutoff < 1.0 && axis != Vec3::ZERO {
                        add(&mut lines, id_color(m as u32), &|l| {
                            cone_lines(
                                l,
                                place(b.center),
                                axis,
                                b.cone_cutoff.max(0.0).asin(),
                                b.radius * scale,
                            )
                        });
                    }
                }
            }
        }
        if lines.is_empty() {
            continue;
        }
        let mesh = meshes.add(
            Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, lines)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors),
        );
        owned_meshes.push(mesh.clone());
        overlays.push(
            commands
                .spawn((
                    CompiledPart,
                    CompiledOverlay(i),
                    Mesh3d(mesh),
                    MeshMaterial3d(unlit.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id(),
        );
    }

    CompiledScene {
        colored,
        wire,
        center,
        scale,
        part_meshes,
        part_corners,
        parts,
        overlays,
        meshes: owned_meshes,
        materials: owned_materials,
        images: owned_images,
    }
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

/// The ground grid of the model whose fit it hangs from.
#[derive(Component)]
struct GroundGrid {
    fit: Entity,
}

/// Lines of the ground grid of a model whose largest side is `size`, with their colors; see
/// docs/features/models.md#ground-grid.
fn ground_grid(size: f32) -> (Vec<[f32; 3]>, Vec<[f32; 4]>) {
    let size = if size > 0.0 { size } else { 1.0 };
    let cell = 10f32.powf((size / 4.0).log10().floor());
    let n = ((size / cell).ceil() as i32).clamp(1, 50);
    let reach = n as f32 * cell;
    let color = |c: bevy_egui::egui::Color32, alpha: f32| {
        let [r, g, b, _] = c.to_array();
        Color::srgba_u8(r, g, b, (alpha * 255.0).round() as u8)
            .to_linear()
            .to_f32_array()
    };
    let (mut lines, mut colors) = (Vec::new(), Vec::new());
    for k in -n..=n {
        let at = k as f32 * cell;
        // Along X (at z = at), then along Z (at x = at).
        lines.extend([[-reach, 0.0, at], [reach, 0.0, at]]);
        colors.extend(
            [if k == 0 {
                color(crate::theme::CHANNEL_R, 0.8)
            } else {
                color(crate::theme::TEXT_WEAK, 0.35)
            }; 2],
        );
        lines.extend([[at, 0.0, -reach], [at, 0.0, reach]]);
        colors.extend(
            [if k == 0 {
                color(crate::theme::CHANNEL_B, 0.8)
            } else {
                color(crate::theme::TEXT_WEAK, 0.35)
            }; 2],
        );
    }
    (lines, colors)
}

/// Keeps a [`GroundGrid`] under the shown model's fit, shown while [`Preview3d::grid`] is on.
fn sync_grid(
    mut commands: Commands,
    preview: Res<Preview3d>,
    loaded: Res<LoadedModel>,
    mut grids: Query<(Entity, &GroundGrid, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut material: Local<Option<Handle<StandardMaterial>>>,
) {
    let model = loaded.0.as_ref().filter(|m| m.fitted.is_some());
    let mut found = false;
    for (entity, grid, mut visibility) in &mut grids {
        if model.is_none_or(|m| m.fit != grid.fit) {
            commands.entity(entity).despawn();
            continue;
        }
        found = true;
        let wanted = if preview.grid {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    let Some(model) = model.filter(|_| !found) else {
        return;
    };
    let Some(fitted) = model.fitted else {
        return;
    };
    let scale = fitted.scale.max_element();
    let size = if scale > 0.0 { MODEL_SIZE / scale } else { 1.0 };
    let (lines, colors) = ground_grid(size);
    let mesh = meshes.add(
        Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, lines)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors),
    );
    let material = material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })
        })
        .clone();
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        if preview.grid {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
        ChildOf(model.fit),
        GroundGrid { fit: model.fit },
        bevy::camera::visibility::NoFrustumCulling,
    ));
}

/// Pauses, resumes and seeks the shown model's animation, and reports its time; see
/// docs/features/animation-events.md#timeline.
fn sync_playback(
    mut preview: ResMut<Preview3d>,
    loaded: Res<LoadedModel>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let seek = preview.animation_seek.take();
    let mut time = None;
    if let Some(model) = &loaded.0
        && let (Some(index), Some((_, nodes))) = (model.playing, &model.graph)
        && let Some(&node) = nodes.get(index)
    {
        for entity in children.iter_descendants(model.fit) {
            let Ok(mut player) = players.get_mut(entity) else {
                continue;
            };
            let Some(active) = player.animation_mut(node) else {
                continue;
            };
            if active.is_paused() != preview.animation_paused {
                if preview.animation_paused {
                    active.pause();
                } else {
                    active.resume();
                }
            }
            if let Some(t) = seek {
                active.set_seek_time(t.max(0.0));
            }
            time.get_or_insert(active.seek_time());
        }
    }
    if preview.animation_time != time {
        preview.animation_time = time;
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

/// Writes [`Preview3d::frames`] for the shown model, `None` while there is none or it is not
/// fitted yet.
fn report_frames(
    loaded: Res<LoadedModel>,
    mut preview: ResMut<Preview3d>,
    children: Query<&Children>,
    skins: Query<&SkinnedMesh>,
    nodes: Query<(&GlobalTransform, Option<&Name>)>,
) {
    // Once shown: the frame it is fitted, joints are not placed by the fit yet.
    let shown = preview.model_status == ModelStatus::Ready;
    preview.frames = loaded.0.as_ref().filter(|_| shown).and_then(|model| {
        model.fitted?;
        let (fit, _) = nodes.get(model.fit).ok()?;
        let skin = children
            .iter_descendants(model.fit)
            .find_map(|entity| skins.get(entity).ok());
        let joints = skin.map_or_else(Vec::new, |skin| {
            skin.joints
                .iter()
                .filter_map(|&joint| {
                    let (global, name) = nodes.get(joint).ok()?;
                    Some((name?.to_string(), global.affine()))
                })
                .collect()
        });
        Some(ModelFrames {
            model: fit.affine(),
            joints,
        })
    });
}

/// The root of a model shown at a locator.
#[derive(Component)]
struct AttachmentRoot;

/// An attachment as spawned.
struct Spawned {
    path: PathBuf,
    parent: Entity,
    entity: Entity,
    gltf: Handle<Gltf>,
    /// Its scene was spawned.
    scene: bool,
}

/// The entity of the shown model's first skin's joint named `name`.
fn find_joint(
    fit: Entity,
    name: &str,
    children: &Query<&Children>,
    skins: &Query<&SkinnedMesh>,
    names: &Query<&Name>,
) -> Option<Entity> {
    let skin = children
        .iter_descendants(fit)
        .find_map(|entity| skins.get(entity).ok())?;
    skin.joints
        .iter()
        .copied()
        .find(|&joint| names.get(joint).is_ok_and(|n| n.as_str() == name))
}

type AttachmentQueries<'w, 's> = (
    Query<'w, 's, &'static Children>,
    Query<'w, 's, &'static SkinnedMesh>,
    Query<'w, 's, &'static Name>,
    Query<'w, 's, (), With<AttachmentRoot>>,
);

/// Spawns, moves and removes the models shown at locators to match
/// [`Preview3d::attachments`]; see docs/features/locators.md#attachment-preview.
#[allow(clippy::too_many_arguments)]
fn sync_attachments(
    mut commands: Commands,
    preview: Res<Preview3d>,
    loaded: Res<LoadedModel>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut spawned: Local<Vec<Spawned>>,
    (children, skins, names, roots): AttachmentQueries,
    mut transforms: Query<(&mut Transform, &mut Visibility), With<AttachmentRoot>>,
    globals: Query<&GlobalTransform>,
) {
    let fit = loaded
        .0
        .as_ref()
        .filter(|m| m.ready && m.fitted.is_some() && preview.viewport.is_some())
        .map(|m| m.fit);
    let mut kept = Vec::new();
    let mut old: Vec<Option<Spawned>> = spawned.drain(..).map(Some).collect();
    for (i, wanted) in preview.attachments.iter().enumerate() {
        let Some(fit) = fit else { break };
        let parent = match &wanted.joint {
            None => Some(fit),
            Some(name) => find_joint(fit, name, &children, &skins, &names),
        };
        let Some(parent) = parent else { continue };
        let reuse = old.get_mut(i).and_then(|slot| {
            slot.take_if(|s| {
                s.path == wanted.path && s.parent == parent && roots.contains(s.entity)
            })
        });
        let mut item = reuse.unwrap_or_else(|| {
            let entity = commands
                .spawn((
                    wanted.transform,
                    Visibility::default(),
                    ChildOf(parent),
                    AttachmentRoot,
                ))
                .id();
            let path = AssetPath::from(wanted.path.clone()).with_source(INPUT_SOURCE);
            Spawned {
                path: wanted.path.clone(),
                parent,
                entity,
                gltf: server.load(path),
                scene: false,
            }
        });
        // Held by its own locator: that locator's model-space transform undone, once its joint
        // is loaded.
        let placed = match &wanted.held {
            None => Some(wanted.transform),
            Some((joint, local)) => {
                let in_model = match joint {
                    None => Some(Affine3A::IDENTITY),
                    Some(name) => find_joint(item.entity, name, &children, &skins, &names)
                        .and_then(|joint| {
                            let root = globals.get(item.entity).ok()?.affine();
                            Some(root.inverse() * globals.get(joint).ok()?.affine())
                        }),
                };
                in_model.map(|m| {
                    let held = m * local.compute_affine();
                    Transform::from_matrix(Mat4::from(
                        wanted.transform.compute_affine() * held.inverse(),
                    ))
                })
            }
        };
        if let Ok((mut transform, mut visibility)) = transforms.get_mut(item.entity) {
            if let Some(placed) = placed
                && *transform != placed
            {
                *transform = placed;
            }
            let shown = if placed.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            visibility.set_if_neq(shown);
        }
        if !item.scene
            && let Some(gltf) = gltfs.get(&item.gltf)
            && let Some(scene) = gltf
                .default_scene
                .clone()
                .or_else(|| gltf.scenes.first().cloned())
        {
            commands.entity(item.entity).insert(WorldAssetRoot(scene));
            item.scene = true;
        }
        kept.push(item);
    }
    for gone in old.into_iter().flatten() {
        if roots.contains(gone.entity) {
            commands.entity(gone.entity).despawn();
        }
    }
    *spawned = kept;
}

/// A skinned mesh's skin and each vertex's joints and weights.
type PosedSkin<'a> = (&'a SkinnedMesh, &'a [[u16; 4]], &'a [[f32; 4]]);

/// A mesh of the shown model as posed this frame.
struct PosedMesh<'a> {
    /// Its glTF mesh and primitive.
    id: Option<(u32, u32)>,
    world: Vec<Vec3>,
    /// Triangle lists, as indices into `world`.
    indices: Vec<usize>,
    /// The skin, with each vertex's joints and weights.
    skin: Option<PosedSkin<'a>>,
}

/// A copy of one of the shown model's meshes drawn in its regions' colors (or a color set's),
/// beside the mesh it stands in for; see docs/features/regions.md#3d-view and
/// docs/features/vertex-colors.md#3d-view.
#[derive(Component)]
struct RegionOverlay {
    source: Entity,
    /// What its colors show: the triangles' region colors, or a color view's generation.
    shows: Shows,
}

#[derive(PartialEq)]
enum Shows {
    Regions(Vec<[u8; 3]>),
    /// Smooth shaded, unlike regions.
    Colors(u64),
}

/// Triangles in no visible region.
const NO_REGION: [u8; 3] = [0x80, 0x80, 0x80];

/// Each triangle's color of glTF primitive `id` with `count` triangles: the last wanted
/// region covering it.
fn region_colors(wants: &[RegionWant], id: (u32, u32), count: usize) -> Vec<[u8; 3]> {
    let mut colors = vec![NO_REGION; count];
    let mut wants: Vec<&RegionWant> = wants
        .iter()
        .filter(|w| (w.mesh, w.primitive) == id)
        .collect();
    wants.sort_by_key(|w| w.region);
    for want in wants {
        match &want.triangles {
            None => colors.fill(want.color),
            Some(some) => {
                for &t in some {
                    if let Some(c) = colors.get_mut(t as usize) {
                        *c = want.color;
                    }
                }
            }
        }
    }
    colors
}

/// Three vertex colors (linear) per triangle.
fn color_attribute(colors: &[[u8; 3]]) -> Vec<[f32; 4]> {
    colors
        .iter()
        .flat_map(|&[r, g, b]| {
            let linear = Color::srgb_u8(r, g, b).to_linear().to_f32_array();
            [linear; 3]
        })
        .collect()
}

/// A color view's colors of glTF primitive `id` with `count` corners; white where it has none.
fn corner_attribute(view: &ColorView, id: (u32, u32), count: usize) -> Vec<[f32; 4]> {
    let corners = view
        .corners
        .iter()
        .find(|(k, _)| *k == id)
        .map_or(&[][..], |(_, c)| c.as_slice());
    (0..count)
        .map(|i| {
            corners
                .get(i)
                .map_or([1.0; 4], |c| c.map(|v| f32::from(v) / 255.0))
        })
        .collect()
}

/// Shows [`RegionOverlay`]s in place of the shown model's meshes while
/// [`Preview3d::region_view`] is on or a [`Preview3d::color_view`] is set, and the meshes
/// again when neither is.
#[allow(clippy::too_many_arguments)]
fn sync_region_view(
    mut commands: Commands,
    preview: Res<Preview3d>,
    loaded: Res<LoadedModel>,
    children: Query<&Children>,
    roots: Query<Entity, With<AttachmentRoot>>,
    sources: Query<(&Mesh3d, &Transform, &ChildOf, Option<&SkinnedMesh>), Without<RegionOverlay>>,
    mut overlays: Query<(Entity, &mut RegionOverlay, &Mesh3d, &mut Transform)>,
    mut visibilities: Query<&mut Visibility>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut material: Local<Option<Handle<StandardMaterial>>>,
) {
    let view = preview.color_view.as_deref();
    let model = loaded
        .0
        .as_ref()
        .filter(|m| m.ready && m.fitted.is_some() && (preview.region_view || view.is_some()));
    // Overlays no longer wanted, whose mesh is gone, or shaded the other way.
    for (entity, overlay, _, _) in &overlays {
        let smooth = matches!(overlay.shows, Shows::Colors(_));
        if model.is_none() || !sources.contains(overlay.source) || smooth != view.is_some() {
            commands.entity(entity).despawn();
            if let Ok(mut visibility) = visibilities.get_mut(overlay.source) {
                *visibility = Visibility::Inherited;
            }
        }
    }
    let Some(model) = model else {
        return;
    };
    let material = material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                perceptual_roughness: 0.9,
                ..default()
            })
        })
        .clone();
    let attached: std::collections::HashSet<Entity> = roots
        .iter()
        .flat_map(|root| std::iter::once(root).chain(children.iter_descendants(root)))
        .collect();
    for entity in children.iter_descendants(model.fit) {
        if attached.contains(&entity) {
            continue;
        }
        let Ok((mesh3d, transform, parent, skin)) = sources.get(entity) else {
            continue;
        };
        let Some(id) = primitive_id(&mesh3d.0) else {
            continue;
        };
        let existing = overlays.iter_mut().find(|(_, overlay, _, _)| {
            overlay.source == entity && matches!(overlay.shows, Shows::Colors(_)) == view.is_some()
        });
        match existing {
            Some((_, mut overlay, overlay_mesh, mut overlay_transform)) => {
                if *overlay_transform != *transform {
                    *overlay_transform = *transform;
                }
                let Some(mut mesh) = meshes.get_mut(&overlay_mesh.0) else {
                    continue;
                };
                let count = mesh.count_vertices();
                match view {
                    Some(view) => {
                        if overlay.shows != Shows::Colors(view.generation) {
                            let colors = corner_attribute(view, id, count);
                            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
                            overlay.shows = Shows::Colors(view.generation);
                        }
                    }
                    None => {
                        let colors = region_colors(&preview.region_wants, id, count / 3);
                        let shows = Shows::Regions(colors);
                        if overlay.shows != shows {
                            let Shows::Regions(colors) = &shows else {
                                unreachable!()
                            };
                            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color_attribute(colors));
                            overlay.shows = shows;
                        }
                    }
                }
            }
            None => {
                let Some(source) = meshes.get(&mesh3d.0) else {
                    continue;
                };
                if source.has_morph_targets()
                    || source.primitive_topology() != PrimitiveTopology::TriangleList
                {
                    continue;
                }
                let mut mesh = source.clone();
                mesh.duplicate_vertices();
                mesh.remove_indices();
                mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
                if view.is_none() || !mesh.contains_attribute(Mesh::ATTRIBUTE_NORMAL) {
                    mesh.compute_flat_normals();
                }
                let count = mesh.count_vertices();
                let shows = match view {
                    Some(view) => {
                        let colors = corner_attribute(view, id, count);
                        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
                        Shows::Colors(view.generation)
                    }
                    None => {
                        let colors = region_colors(&preview.region_wants, id, count / 3);
                        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, color_attribute(&colors));
                        Shows::Regions(colors)
                    }
                };
                let mut overlay = commands.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material.clone()),
                    *transform,
                    ChildOf(parent.parent()),
                    RegionOverlay {
                        source: entity,
                        shows,
                    },
                ));
                if let Some(skin) = skin {
                    overlay.insert((skin.clone(), bevy::camera::visibility::NoFrustumCulling));
                }
                if let Ok(mut visibility) = visibilities.get_mut(entity) {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

/// The glTF mesh and primitive of a mesh loaded from a glTF (`Mesh<m>/Primitive<p>`).
fn primitive_id(mesh: &Handle<Mesh>) -> Option<(u32, u32)> {
    let label = mesh.path()?.label()?;
    let (m, p) = label.strip_prefix("Mesh")?.split_once("/Primitive")?;
    Some((m.parse().ok()?, p.parse().ok()?))
}

type MeshQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Mesh3d,
        &'static GlobalTransform,
        Option<&'static SkinnedMesh>,
        Has<RegionOverlay>,
    ),
>;

/// Calls `visit` with each mesh of the shown model (not its attachments) that `wanted`
/// accepts by its glTF id, skinned ones through their joints.
#[allow(clippy::too_many_arguments)]
fn posed_meshes<'a>(
    fit: Entity,
    children: &Query<&Children>,
    mesh_entities: &'a MeshQuery,
    nodes: &Query<(&GlobalTransform, Option<&Name>)>,
    roots: &Query<Entity, With<AttachmentRoot>>,
    meshes: &'a Assets<Mesh>,
    bindposes: &Assets<SkinnedMeshInverseBindposes>,
    wanted: impl Fn(Option<(u32, u32)>) -> bool,
    mut visit: impl FnMut(PosedMesh<'a>),
) {
    let attached: std::collections::HashSet<Entity> = roots
        .iter()
        .flat_map(|root| std::iter::once(root).chain(children.iter_descendants(root)))
        .collect();
    for entity in children.iter_descendants(fit) {
        if attached.contains(&entity) {
            continue;
        }
        let Ok((mesh3d, global, skin, false)) = mesh_entities.get(entity) else {
            continue;
        };
        let id = primitive_id(&mesh3d.0);
        if !wanted(id) {
            continue;
        }
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue;
        };
        let Some(positions) = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
        else {
            continue;
        };
        let joint_indices = match mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX) {
            Some(bevy::mesh::VertexAttributeValues::Uint16x4(v)) => Some(v.as_slice()),
            _ => None,
        };
        let joint_weights = match mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT) {
            Some(bevy::mesh::VertexAttributeValues::Float32x4(v)) => Some(v.as_slice()),
            _ => None,
        };
        let skin = skin.zip(joint_indices.zip(joint_weights));
        // Joint matrices of a skinned mesh: joint world × inverse bind.
        let matrices: Option<Vec<Mat4>> = skin.and_then(|(skin, _)| {
            let binds = bindposes.get(&skin.inverse_bindposes)?;
            Some(
                skin.joints
                    .iter()
                    .zip(binds.iter())
                    .map(|(&joint, bind)| {
                        nodes
                            .get(joint)
                            .map_or(Mat4::IDENTITY, |(g, _)| g.to_matrix() * *bind)
                    })
                    .collect(),
            )
        });
        let world = positions
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let p = Vec3::from(*p);
                match (&matrices, skin) {
                    (Some(matrices), Some((_, (indices, weights)))) => (0..4)
                        .map(|k| {
                            let m = matrices.get(indices[i][k] as usize).copied();
                            m.unwrap_or(Mat4::IDENTITY).transform_point3(p) * weights[i][k]
                        })
                        .sum(),
                    _ => global.transform_point(p),
                }
            })
            .collect::<Vec<Vec3>>();
        let indices: Vec<usize> = match mesh.indices() {
            Some(indices) => indices.iter().collect(),
            None => (0..world.len()).collect(),
        };
        visit(PosedMesh {
            id,
            world,
            indices,
            skin: skin
                .filter(|_| matrices.is_some())
                .map(|(skin, (ji, jw))| (skin, ji, jw)),
        });
    }
}

/// Answers [`Preview3d::surface_ray`] with the nearest triangle of the shown model (not its
/// attachments), skinned meshes as posed; see docs/features/locators.md#place-on-surface and
/// docs/features/regions.md#painting.
#[allow(clippy::too_many_arguments)]
fn cast_surface(
    loaded: Res<LoadedModel>,
    mut preview: ResMut<Preview3d>,
    children: Query<&Children>,
    mesh_entities: MeshQuery,
    nodes: Query<(&GlobalTransform, Option<&Name>)>,
    roots: Query<Entity, With<AttachmentRoot>>,
    meshes: Res<Assets<Mesh>>,
    bindposes: Res<Assets<SkinnedMeshInverseBindposes>>,
) {
    let Some(ray) = preview.surface_ray.take() else {
        return;
    };
    let Some(model) = loaded.0.as_ref().filter(|m| m.fitted.is_some()) else {
        preview.surface_hit = Some(None);
        return;
    };
    let mut posed = Vec::new();
    posed_meshes(
        model.fit,
        &children,
        &mesh_entities,
        &nodes,
        &roots,
        &meshes,
        &bindposes,
        |_| true,
        |mesh| posed.push(mesh),
    );
    // The nearest hit: mesh, triangle, distance.
    let mut best: Option<(usize, usize, f32)> = None;
    for (m, mesh) in posed.iter().enumerate() {
        for (t, triangle) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            let [a, b, c] = triangle.map(|i| mesh.world.get(i).copied().unwrap_or(Vec3::NAN));
            if let Some(d) = ray_triangle(ray.origin, ray.dir, a, b, c)
                && best.is_none_or(|(_, _, nearest)| d < nearest)
            {
                best = Some((m, t, d));
            }
        }
    }
    let corners = |mesh: &PosedMesh, t: usize| {
        let triangle = mesh.indices.as_chunks::<3>().0[t];
        triangle.map(|i| mesh.world.get(i).copied().unwrap_or(Vec3::NAN))
    };
    preview.surface_hit = Some(best.map(|(m, t, d)| {
        let mesh = &posed[m];
        let point = ray.origin + ray.dir * d;
        let [a, b, c] = corners(mesh, t);
        let mut normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.dot(ray.dir) > 0.0 {
            normal = -normal;
        }
        // The named joint weighing most on the triangle.
        let joint = mesh.skin.and_then(|(skin, ji, jw)| {
            let mut sums: Vec<(u16, f32)> = Vec::new();
            for &v in &mesh.indices.as_chunks::<3>().0[t] {
                for k in 0..4 {
                    let (j, w) = (ji[v][k], jw[v][k]);
                    match sums.iter_mut().find(|(sj, _)| *sj == j) {
                        Some((_, sw)) => *sw += w,
                        None => sums.push((j, w)),
                    }
                }
            }
            let (j, _) = sums.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
            let (_, name) = nodes.get(*skin.joints.get(j as usize)?).ok()?;
            Some(name?.to_string())
        });
        let triangle = mesh.id.map(|(m, p)| (m, p, t as u32));
        // The brush: centers within reach of the hit, on triangles facing the ray.
        let mut brush: Vec<TriangleId> = triangle.into_iter().collect();
        if ray.brush > 0.0 {
            for other in &posed {
                let Some((m, p)) = other.id else {
                    continue;
                };
                for u in 0..other.indices.len() / 3 {
                    let [a, b, c] = corners(other, u);
                    let facing = (b - a).cross(c - a).dot(ray.dir) < 0.0;
                    let id = (m, p, u as u32);
                    if facing
                        && ((a + b + c) / 3.0).distance(point) <= ray.brush
                        && Some(id) != triangle
                    {
                        brush.push(id);
                    }
                }
            }
        }
        SurfaceHit {
            point,
            normal,
            joint,
            brush,
        }
    }));
}

/// Answers [`Preview3d::region_wants`] with the wanted triangles as posed; see
/// docs/features/regions.md#3d-view.
#[allow(clippy::too_many_arguments)]
fn report_regions(
    loaded: Res<LoadedModel>,
    mut preview: ResMut<Preview3d>,
    children: Query<&Children>,
    mesh_entities: MeshQuery,
    nodes: Query<(&GlobalTransform, Option<&Name>)>,
    roots: Query<Entity, With<AttachmentRoot>>,
    meshes: Res<Assets<Mesh>>,
    bindposes: Res<Assets<SkinnedMeshInverseBindposes>>,
) {
    let mut out = Vec::new();
    if let Some(model) = loaded.0.as_ref().filter(|m| m.fitted.is_some())
        && preview.model_status == ModelStatus::Ready
        && let Some(outline) = preview.region_outline
    {
        let wants: Vec<&RegionWant> = preview
            .region_wants
            .iter()
            .filter(|w| w.region == outline)
            .collect();
        posed_meshes(
            model.fit,
            &children,
            &mesh_entities,
            &nodes,
            &roots,
            &meshes,
            &bindposes,
            |id| id.is_some_and(|(m, p)| wants.iter().any(|w| w.mesh == m && w.primitive == p)),
            |mesh| {
                let Some((m, p)) = mesh.id else {
                    return;
                };
                let triangles = mesh.indices.as_chunks::<3>().0;
                let corners =
                    |t: &[usize; 3]| t.map(|i| mesh.world.get(i).copied().unwrap_or(Vec3::NAN));
                for want in wants.iter().filter(|w| w.mesh == m && w.primitive == p) {
                    match &want.triangles {
                        None => out.extend(triangles.iter().map(|t| (want.region, corners(t)))),
                        Some(some) => out.extend(
                            some.iter()
                                .filter_map(|&t| triangles.get(t as usize))
                                .map(|t| (want.region, corners(t))),
                        ),
                    }
                }
            },
        );
    }
    preview.region_triangles = out;
}

/// Distance along the ray to triangle `a b c` (either side), Möller–Trumbore.
fn ray_triangle(origin: Vec3, dir: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lut::Lut;

    #[test]
    fn rays_hit_triangles_in_front() {
        let (a, b, c) = (Vec3::ZERO, Vec3::X, Vec3::Y);
        let t = ray_triangle(Vec3::new(0.2, 0.2, 5.0), Vec3::NEG_Z, a, b, c);
        assert_eq!(t, Some(5.0));
        // Behind the origin, or beside the triangle.
        assert_eq!(
            ray_triangle(Vec3::new(0.2, 0.2, 5.0), Vec3::Z, a, b, c),
            None
        );
        assert_eq!(
            ray_triangle(Vec3::new(0.9, 0.9, 5.0), Vec3::NEG_Z, a, b, c),
            None
        );
    }

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
        assert!(t.up().y > 0.0);
    }

    #[test]
    fn grids_fit_the_model() {
        // A 155-unit model gets 10-unit cells out to 160 each way, a 1.6-unit one 0.1.
        let (lines, colors) = ground_grid(155.0);
        assert_eq!(lines.len(), (2 * 16 + 1) * 4);
        assert_eq!(colors.len(), lines.len());
        assert_eq!(lines[0], [-160.0, 0.0, -160.0]);
        let (lines, _) = ground_grid(1.6);
        assert!((lines[0][0] + 1.6).abs() < 1e-4);
        // Never more than 50 cells each way.
        assert_eq!(ground_grid(1e6).0.len(), (2 * 10 + 1) * 4);
    }

    #[test]
    fn axis_views_look_at_the_center() {
        let mut orbit = Orbit::default();
        for (axis, positive, from) in [
            (0, true, Vec3::X),
            (0, false, Vec3::NEG_X),
            (1, true, Vec3::Y),
            (1, false, Vec3::NEG_Y),
            (2, true, Vec3::Z),
            (2, false, Vec3::NEG_Z),
        ] {
            orbit.look_from(axis, positive);
            let t = orbit.transform();
            assert!((t.translation.normalize() - from).length() < 1e-4, "{from}");
            assert!((t.forward().as_vec3() + from).length() < 1e-4, "{from}");
        }
        // From above, +X points right and +Z down the screen.
        orbit.look_from(1, true);
        let t = orbit.transform();
        assert!((t.right().as_vec3() - Vec3::X).length() < 1e-4);
        assert!((t.up().as_vec3() - Vec3::NEG_Z).length() < 1e-4);
        // Dragging from there stays within reach of it, then within the limit.
        orbit.drag(Vec2::new(0.0, 10.0));
        assert!((orbit.pitch - 90f32.to_radians()).abs() < 1e-5);
        orbit.drag(Vec2::new(0.0, -100.0));
        assert!(orbit.pitch < 85f32.to_radians());
    }
}
