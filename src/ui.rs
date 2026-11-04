use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{
    Align, Align2, FontId, Frame, Layout, Margin, PointerButton, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::amesh::{MeshOptions, MeshSettings};
use crate::amod::{self, Amod};
use crate::aschema::{self, Body, Schema};
use crate::assets::Assets;
use crate::build;
use crate::color_picker::{self, Picker};
use crate::config::Project;
use crate::enrich::{self, PropValue, Properties, Region, ShapeType};
use crate::fileops;
use crate::gizmo;
use crate::gradient::{
    self, CellGrid, Dither, Gradient, Gradients, Layout as GradientLayout, Stop,
};
use crate::icons::Icon;
use crate::inspect::{self, Inspection, Location};
use crate::logo;
use crate::lut::Lut;
use crate::manifest::Kind;
use crate::model;
use crate::noise::{GeneratorKind, Modifier, ModifierKind, Noise, ParamKind, ParamSpec, Value};
use crate::palette::{self, Palette, Swatch};
use crate::presets::{self, Inherited};
use crate::preview3d::{
    CompiledUpload, CubemapUpload, MeshDisplay, MeshOverlay, ModelFrames, ModelStatus, Orbit,
    Preview3d, Shape, Upload,
};
use crate::texture::Image;
use crate::theme::{
    self, ACCENT, BG_BAR, BG_CENTER, BG_HEADER, BG_PANEL, CHANNEL_B, CHANNEL_G, CHANNEL_R,
    CHECKER_DARK, CHECKER_LIGHT, STROKE, TEXT, TEXT_WEAK,
};
use crate::tree::{FileTrees, Listing, Node};
use crate::vertex_colors;

mod anim;
mod config_dialog;
mod curve;
mod data;
mod graph;
mod material_edit;
mod pack_view;
mod schema;
mod structs_edit;
mod tree_nav;
use crate::agraph::Graph;
use crate::vision::Vision;
use crate::{chart, preview3d};
use anim::{AnimPlayer, canvas_compiled_animation};
use config_dialog::{ConfigDialog, DialogEnd, config_dialog};
use data::{DataEdit, PreparedData, canvas_data, instances_tab};
use graph::{GraphEdit, GraphView, canvas_graph, graph_tab};
use material_edit::{MaterialEdit, PreparedMaterial, canvas_material, material_tab};
use pack_view::{PackView, pack_view};
use schema::{SchemaEdit, canvas_schema, types_tab};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let [r, g, b, _] = BG_CENTER.to_array();
        app.insert_resource(ClearColor(Color::srgb_u8(r, g, b)))
            .init_resource::<UiState>()
            .add_systems(Startup, scan_trees)
            .add_systems(EguiPrimaryContextPass, shell);
    }
}

#[derive(Resource)]
struct UiState {
    inspector_tab: InspectorTab,
    browser_tab: BrowserTab,
    rescan: bool,
    start_build: bool,
    /// Pack after the build started by `start_build`.
    pack: bool,
    /// Open the Config dialog.
    open_config: bool,
    config: Option<ConfigDialog>,
    build: BuildStatus,
    selection: Option<Selection>,
    /// Set when the selection changed or the trees were rescanned.
    reinspect: bool,
    inspector: Inspector,
    inspected: Option<Inspected>,
    view: View,
    mode: ViewMode,
    lut: LutUi,
    last_image: Option<LastImage>,
    /// Open folders in the file browser.
    open: HashSet<PathBuf>,
    rename: Option<Rename>,
    confirm_delete: Option<PathBuf>,
    /// A failed file operation, shown in the status bar until the next operation or build.
    notice: Option<String>,
    /// Canvas toolbar state of each image, by path.
    canvas_options: HashMap<PathBuf, CanvasOptions>,
    /// Animation chosen for each model, by path; `None` is the rest pose.
    animations: HashMap<PathBuf, Option<usize>>,
    /// Mip level shown for each compiled texture, by path.
    mip_levels: HashMap<PathBuf, usize>,
    /// How compiled meshes are drawn: display mode and bounds overlays.
    mesh_view: MeshView,
    /// Level of detail shown for each compiled mesh, by path.
    lod_levels: HashMap<PathBuf, usize>,
    /// The color set each compiled mesh shows, by path; see
    /// docs/features/vertex-colors.md#compiled-meshes.
    mesh_colors: HashMap<PathBuf, Option<usize>>,
    /// The type each schema's editor shows, by path; see docs/features/schemas.md#types-tab.
    schema_types: HashMap<PathBuf, usize>,
    /// The instance each data file's editor shows, by path; see
    /// docs/features/instances.md#instances-tab.
    data_instances: HashMap<PathBuf, usize>,
    /// What the last build reported; see docs/features/gui.md#build-marks.
    build_marks: BuildMarks,
    /// The Structs tab's state, by file; see docs/features/structs.md#structs-tab.
    structs_views: HashMap<PathBuf, structs_edit::StructsView>,
    /// Each graph's view, by path; see docs/features/graphs.md#canvas.
    graph_views: HashMap<PathBuf, GraphView>,
    /// The animation timeline under a model; see docs/features/animation-events.md#timeline.
    timeline: Timeline,
    /// Compiled meshes in 3D (else their chunks), and the chunk shown.
    mesh_in_3d: bool,
    /// The chosen chunk of the Chunks view, of a compiled mesh or font.
    mesh_chunk: String,
    /// A compiled font shows its atlas, else its chunks.
    font_atlas: bool,
    /// A compiled animation shows its chunks, else plays.
    anim_chunks: bool,
    /// The Manifest view's filter.
    data_filter: String,
    /// The color vision the canvas simulates; `None` is normal vision.
    vision: Option<Vision>,
    /// The model preview's locator tools.
    locators: LocatorTools,
    /// A model's sidecar was saved: its next inspection keeps the 3D view's model loaded.
    keep_model: bool,
    /// The Open source dialog is open.
    licenses: bool,
    /// Where the last mouse press landed: that area's keys act.
    keys: KeyArea,
    /// The selection moved by key: its tree row scrolls into view on the next frame.
    tree_scroll: bool,
    /// The HDR map (input-relative) lighting material previews; `None` is the studio light.
    material_light: Option<String>,
    /// What the canvas shows of a pack; see docs/features/pack.md#views.
    pack_view: PackView,
}

/// The areas keys belong to; see KEYS.md.
#[derive(Clone, Copy, PartialEq, Default)]
enum KeyArea {
    #[default]
    Tree,
    Canvas,
    Inspector,
}

/// Gives `area` (the panel `ui` fills) the keys when a mouse press lands in it, not on a
/// popup over it.
fn claim_keys(ui: &Ui, keys: &mut KeyArea, area: KeyArea) {
    let rect = ui.max_rect();
    let pressed = ui.input(|i| {
        i.pointer
            .any_pressed()
            .then(|| i.pointer.interact_pos())
            .flatten()
    });
    if let Some(p) = pressed
        && rect.contains(p)
        && ui
            .ctx()
            .layer_id_at(p)
            .is_none_or(|l| l.order == egui::Order::Background)
    {
        *keys = area;
    }
}

/// The model preview's locator, shape and region tools, for the session; see
/// docs/features/locators.md#3d-view and docs/features/regions.md#painting.
struct LocatorTools {
    /// The gizmo on the chosen locator (or shape), when on.
    gizmo: Option<gizmo::Mode>,
    /// A gizmo handle being dragged.
    drag: Option<gizmo::Drag>,
    /// Place on surface is armed: the next click on the model adds a locator there.
    armed: bool,
    /// A Place click waits for its surface hit.
    awaiting: bool,
    /// Locators, shapes and regions hidden in the 3D view, by model path and name.
    hidden: HashMap<PathBuf, HashSet<String>>,
    hidden_shapes: HashMap<PathBuf, HashSet<String>>,
    hidden_regions: HashMap<PathBuf, HashSet<String>>,
    hidden_paths: HashMap<PathBuf, HashSet<String>>,
    /// The toolbar's Show group: locators, shapes, regions and paths drawn at all; see
    /// docs/features/models.md#show.
    show: [bool; 5],
    /// Clicks on the model add points to the chosen path; a click's ray waits for its hit.
    draw: bool,
    draw_awaiting: bool,
    /// Where the stroke being drawn last asked for a point.
    draw_last: Option<egui::Pos2>,
    /// The plane Mirror reflects across: 0 X, 1 Y, 2 Z.
    mirror_axis: usize,
    /// The chosen region paints (or, with `rubber`, erases); a stroke's ray waits for its
    /// hit; this frame's stroke erases.
    paint: bool,
    rubber: bool,
    paint_awaiting: bool,
    erase: bool,
    /// The paint brush's radius on screen, in points.
    brush: f32,
    /// The color set shown on the model: its model, what it shows and the colors; see
    /// docs/features/vertex-colors.md#3d-view.
    color_view: Option<(PathBuf, ColorKey, std::sync::Arc<preview3d::ColorView>)>,
    color_generation: u64,
}

/// What a color view shows: the model's whole list of sets and the shown one.
type ColorKey = (Vec<enrich::ColorSet>, usize);

impl Default for LocatorTools {
    fn default() -> Self {
        Self {
            gizmo: None,
            drag: None,
            armed: false,
            awaiting: false,
            hidden: HashMap::new(),
            hidden_shapes: HashMap::new(),
            hidden_regions: HashMap::new(),
            hidden_paths: HashMap::new(),
            show: [true, true, false, true, true],
            draw: false,
            draw_awaiting: false,
            draw_last: None,
            mirror_axis: 0,
            paint: false,
            rubber: false,
            paint_awaiting: false,
            erase: false,
            brush: 12.0,
            color_view: None,
            color_generation: 0,
        }
    }
}

impl LocatorTools {
    /// Turns the gizmo on in `mode`, or off when it already is.
    fn toggle_gizmo(&mut self, mode: gizmo::Mode) {
        self.gizmo = (self.gizmo != Some(mode)).then_some(mode);
    }
}

/// Canvas zoom and pan, see docs/features/canvas.md#view.
#[derive(Clone, Copy)]
struct View {
    /// Screen pixels per image pixel.
    zoom: f32,
    /// Image center relative to the canvas center, in points.
    pan: Vec2,
    /// Still the automatic fit: refit when the canvas or image changes.
    fit: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit: true,
        }
    }
}

const MIN_ZOOM: f32 = 1.0 / 32.0;
const MAX_ZOOM: f32 = 64.0;
const FIT_MARGIN: f32 = 24.0;
/// Zoom factor per wheel notch, and per 50 points of smooth (touchpad) scrolling.
const WHEEL_ZOOM: f32 = 1.2;

#[derive(Clone, PartialEq, Eq)]
struct Selection {
    location: Location,
    path: PathBuf,
}

struct Inspected {
    path: PathBuf,
    inspection: Inspection,
    /// The uploaded image, kept to build single-channel views.
    pixels: Option<Image>,
    /// A compiled texture's mip levels, level 0 first (empty otherwise), and which one
    /// `pixels` / `preview` show.
    levels: Vec<Image>,
    shown_level: usize,
    preview: Option<egui::TextureHandle>,
    /// Single-channel view of `pixels`, built on demand.
    channel_preview: Option<(Channels, egui::TextureHandle)>,
    /// What the shown channels look like to a color vision deficiency, built on demand.
    vision_preview: Option<((Channels, Vision), egui::TextureHandle)>,
    /// Channels last sent to the 3D preview.
    uploaded_3d: Option<Channels>,
    /// Environment map faces, kept to upload to the 3D preview.
    cubemap: Option<Vec<Image>>,
    uploaded_cubemap: bool,
    lut: Option<Lut>,
    /// An `.anoise` file's parameters: (being edited, as saved), or why they can't be read.
    noise: Option<Result<NoiseEdit, String>>,
    /// An `.agradient` file being edited, or why it can't be read.
    gradient: Option<Result<GradientEdit, String>>,
    /// An `.apalette` file being edited, or why it can't be read.
    palette: Option<Result<PaletteEdit, String>>,
    /// An `.aschema` file being edited, or why it can't be read and its text.
    schema: Option<Result<SchemaEdit, (String, String)>>,
    /// An `.agraph` file being edited, or why it can't be read.
    graph: Option<Result<GraphEdit, String>>,
    /// A compiled animation's playback; see docs/features/models.md#compiled-animation-preview.
    anim_player: AnimPlayer,
    /// An atlas folder's settings: (being edited, as saved), or why they can't be read.
    atlas: Option<Result<(crate::atlas::AtlasConfig, crate::atlas::AtlasConfig), String>>,
    /// An `.adata` file being edited with its schema, or why it can't be read and its text.
    data: Option<Result<DataEdit, (String, String)>>,
    /// A model's `.amod` mesh settings being edited, or why they can't be read.
    mesh: Option<Result<MeshEdit, String>>,
    /// An `.amat` being edited, or why it can't be read; and whether the 3D view has its
    /// textures.
    material: Option<Result<MaterialEdit, String>>,
    uploaded_material: bool,
    /// An image's `.amod` (its modifiers) being edited, or why it can't be read.
    sidecar: Option<Result<SidecarEdit, String>>,
    lut_compare: Option<CompareCache>,
    uploaded_color_cube: bool,
    /// A glTF model's input-relative path and this inspection's generation, for the 3D view.
    model: Option<(PathBuf, u64)>,
    /// A compiled mesh's levels and material slots, and the level and color set the 3D view
    /// has.
    compiled_mesh: Option<crate::amesh::Preview>,
    compiled_materials: Vec<Option<inspect::MaterialPreview>>,
    uploaded_lod: Option<(usize, Option<usize>)>,
    compiled_bytes: Option<Vec<u8>>,
    /// A model's geometry, for hulls, shape sizes and region materials.
    geometry: Option<model::ModelGeometry>,
    /// Convex hulls computed for shapes, by joint (`None` for model space).
    hulls: HashMap<Option<String>, Option<crate::hull::Hull>>,
    /// What the Structs tab read; see docs/features/structs.md#structs-tab.
    structs_cache: Option<structs_edit::StructsCache>,
}

/// Before / after textures of the LUT compare view for one source image.
struct CompareCache {
    /// Path of the source image; `None` for the reference chart.
    key: Option<PathBuf>,
    vision: Option<Vision>,
    before: egui::TextureHandle,
    after: egui::TextureHandle,
}

/// The last image selected, for the LUT compare view.
struct LastImage {
    path: PathBuf,
    name: String,
    image: Image,
}

/// LUT view state, see docs/features/lut.md#views.
struct LutUi {
    view: LutView,
    source: LutSource,
    /// Divider position across the image, 0..1.
    split: f32,
    /// Cube view shows the untransformed cube.
    cube_before: bool,
}

impl Default for LutUi {
    fn default() -> Self {
        Self {
            view: LutView::Compare,
            source: LutSource::Chart,
            split: 0.5,
            cube_before: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LutView {
    Compare,
    Cube,
    Curves,
}

impl LutView {
    const ALL: [Self; 3] = [Self::Compare, Self::Cube, Self::Curves];

    fn name(self) -> &'static str {
        match self {
            Self::Compare => "Compare",
            Self::Cube => "Cube",
            Self::Curves => "Curves",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LutSource {
    Chart,
    Image,
}

/// Canvas view mode, see docs/features/canvas.md#view-bar.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum ViewMode {
    #[default]
    TwoD,
    ThreeD,
}

impl ViewMode {
    const ALL: [Self; 2] = [Self::TwoD, Self::ThreeD];

    fn name(self) -> &'static str {
        match self {
            Self::TwoD => "2D",
            Self::ThreeD => "3D",
        }
    }
}

/// Canvas toolbar state of one image, see docs/features/canvas.md#toolbar.
#[derive(Clone, Copy, Default)]
struct CanvasOptions {
    tiled: bool,
    channels: Channels,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Channels {
    #[default]
    Rgb,
    R,
    G,
    B,
    A,
    /// Grayscale images: shown as stored, like `Rgb`.
    Gray,
}

impl Channels {
    fn name(self) -> &'static str {
        match self {
            Self::Rgb => "RGB",
            Self::R => "Red",
            Self::G => "Green",
            Self::B => "Blue",
            Self::A => "Alpha",
            Self::Gray => "Gray",
        }
    }

    /// Index in an RGBA pixel; `None` for all channels.
    fn index(self) -> Option<usize> {
        match self {
            Self::Rgb | Self::Gray => None,
            Self::R => Some(0),
            Self::G => Some(1),
            Self::B => Some(2),
            Self::A => Some(3),
        }
    }
}

enum BuildStatus {
    Idle,
    /// The build (and pack) thread with its final message, when it started, and whether it is
    /// packing by now.
    Running(JoinHandle<(String, BuildMarks)>, Instant, Arc<AtomicBool>),
    Finished(String),
}

/// What the last build could not compile, or compiled with warnings, by input path; see
/// docs/features/gui.md#build-marks.
#[derive(Default)]
struct BuildMarks {
    /// Each marked source's messages, `true` for an error.
    files: HashMap<PathBuf, Vec<(bool, String)>>,
    /// Folders holding marked sources: their errors and warnings below.
    folders: HashMap<PathBuf, (usize, usize)>,
}

impl BuildMarks {
    fn new(input: &Path, outcome: &build::Outcome) -> Self {
        let mut marks = Self::default();
        let found = (outcome.failures.iter().map(|f| (true, f)))
            .chain(outcome.warnings.iter().map(|w| (false, w)));
        for (error, problem) in found {
            let message = format!(
                "{}: {}",
                if error { "error" } else { "warning" },
                problem.message
            );
            // A texture group (`brick/brick.*`) is no one file: its folder is marked.
            let path = input.join(&problem.source);
            let mut at = Vec::new();
            if !problem.source.contains('*') && path.exists() {
                if model::is_model(&path) {
                    at.push(amod::path_for(&path));
                }
                at.push(path.clone());
            }
            for file in &at {
                marks
                    .files
                    .entry(file.clone())
                    .or_default()
                    .push((error, message.clone()));
            }
            for dir in path.ancestors().skip(1) {
                if !dir.starts_with(input) || dir == input {
                    break;
                }
                let count = marks.folders.entry(dir.to_path_buf()).or_default();
                if error {
                    count.0 += 1;
                } else {
                    count.1 += 1;
                }
            }
        }
        // Errors first.
        for messages in marks.files.values_mut() {
            messages.sort_by_key(|(error, _)| !error);
        }
        marks
    }

    /// A row's mark: its color and what hovering it lists.
    fn of(&self, path: &Path) -> Option<(egui::Color32, String)> {
        if let Some(messages) = self.files.get(path) {
            let color = if messages.iter().any(|(e, _)| *e) {
                CHANNEL_R
            } else {
                ACCENT
            };
            let text: Vec<&str> = messages.iter().map(|(_, m)| m.as_str()).collect();
            return Some((color, text.join("\n")));
        }
        let &(errors, warnings) = self.folders.get(path)?;
        let plural = |n: usize, word: &str| match n {
            1 => format!("1 {word}"),
            n => format!("{n} {word}s"),
        };
        let counts: Vec<String> = [(errors, "error"), (warnings, "warning")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, word)| plural(n, word))
            .collect();
        let color = if errors > 0 { CHANNEL_R } else { ACCENT };
        Some((color, format!("{} below", counts.join(", "))))
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            inspector_tab: InspectorTab::Info,
            browser_tab: BrowserTab::Input,
            rescan: false,
            start_build: false,
            pack: false,
            open_config: false,
            config: None,
            build: BuildStatus::Idle,
            selection: None,
            reinspect: false,
            inspector: Inspector::default(),
            inspected: None,
            view: View::default(),
            mode: ViewMode::default(),
            lut: LutUi::default(),
            last_image: None,
            open: HashSet::new(),
            rename: None,
            confirm_delete: None,
            notice: None,
            canvas_options: HashMap::new(),
            animations: HashMap::new(),
            mip_levels: HashMap::new(),
            mesh_view: MeshView::default(),
            lod_levels: HashMap::new(),
            mesh_colors: HashMap::new(),
            schema_types: HashMap::new(),
            data_instances: HashMap::new(),
            build_marks: BuildMarks::default(),
            structs_views: HashMap::new(),
            graph_views: HashMap::new(),
            timeline: Timeline::default(),
            mesh_in_3d: true,
            font_atlas: true,
            data_filter: String::new(),
            vision: None,
            mesh_chunk: "DESC".into(),
            anim_chunks: false,
            locators: LocatorTools::default(),
            keep_model: false,
            licenses: false,
            keys: KeyArea::default(),
            tree_scroll: false,
            material_light: None,
            pack_view: PackView::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BrowserTab {
    Input,
    Output,
}

impl BrowserTab {
    const ALL: [Self; 2] = [Self::Input, Self::Output];

    fn location(self) -> Location {
        match self {
            Self::Input => Location::Input,
            Self::Output => Location::Output,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::Output => "Output",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InspectorTab {
    Info,
    Parameters,
    Locators,
    Shapes,
    Regions,
    Paths,
    VertexColors,
    Properties,
    Modifiers,
    Gradients,
    Colors,
    Types,
    Graph,
    Tree,
    Atlas,
    Instances,
    Material,
    Structs,
    Animations,
    Mipmaps,
    Lods,
    Assets,
}

impl InspectorTab {
    const ALL: [Self; 22] = [
        Self::Info,
        Self::Parameters,
        Self::Locators,
        Self::Shapes,
        Self::Regions,
        Self::Paths,
        Self::VertexColors,
        Self::Properties,
        Self::Modifiers,
        Self::Gradients,
        Self::Colors,
        Self::Types,
        Self::Graph,
        Self::Tree,
        Self::Atlas,
        Self::Instances,
        Self::Material,
        Self::Structs,
        Self::Animations,
        Self::Mipmaps,
        Self::Lods,
        Self::Assets,
    ];

    fn icon(self) -> Icon {
        match self {
            Self::Info => Icon::Info,
            Self::Parameters => Icon::SlidersVertical,
            Self::Locators => Icon::MapPin,
            Self::Shapes => Icon::Shapes,
            Self::Regions => Icon::PaintBucket,
            Self::Paths => Icon::Spline,
            Self::VertexColors => Icon::SwatchBook,
            Self::Properties => Icon::Database,
            Self::Modifiers => Icon::Wrench,
            Self::Gradients | Self::Colors => Icon::Palette,
            Self::Types => Icon::Braces,
            Self::Graph => Icon::GraphStar,
            Self::Tree => Icon::ListTree,
            Self::Atlas => Icon::Map,
            Self::Instances => Icon::Package,
            Self::Material => Icon::Gem,
            Self::Structs => Icon::Boxes,
            Self::Animations => Icon::Film,
            Self::Mipmaps => Icon::Images,
            Self::Lods => Icon::BringToFront,
            Self::Assets => Icon::Files,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Parameters => "Parameters",
            Self::Locators => "Locators",
            Self::Shapes => "Shapes",
            Self::Regions => "Regions",
            Self::Paths => "Paths",
            Self::VertexColors => "Vertex colors",
            Self::Properties => "Properties",
            Self::Modifiers => "Modifiers",
            Self::Gradients => "Gradients",
            Self::Colors => "Colors",
            Self::Types => "Types",
            Self::Graph => "Graph",
            Self::Tree => "Tree",
            Self::Atlas => "Atlas",
            Self::Instances => "Instances",
            Self::Material => "Material",
            Self::Structs => "Structs",
            Self::Animations => "Animations",
            Self::Mipmaps => "Mipmaps",
            Self::Lods => "Levels of detail",
            Self::Assets => "Assets",
        }
    }

    fn enabled(self, selection: Option<&Selection>, inspected: Option<&Inspected>) -> bool {
        match self {
            Self::Info => true,
            Self::Mipmaps => inspected.is_some_and(|i| i.levels.len() > 1),
            Self::Lods => inspected
                .and_then(|i| i.compiled_mesh.as_ref())
                .is_some_and(|m| m.levels.len() > 1),
            Self::Animations => {
                inspected.is_some_and(|i| i.model.is_some() && !i.inspection.animations.is_empty())
            }
            Self::Parameters => selection.is_some_and(|s| {
                s.location == Location::Input
                    && (build::is_noise(&s.path)
                        || gradient::is_gradient(&s.path)
                        || palette::is_palette(&s.path)
                        || model::is_model(&s.path))
                    && s.path.is_file()
            }),
            Self::VertexColors
                if inspected
                    .and_then(|i| i.compiled_mesh.as_ref())
                    .is_some_and(|m| !m.color_sets.is_empty()) =>
            {
                true
            }
            Self::Locators
            | Self::Shapes
            | Self::Regions
            | Self::Paths
            | Self::VertexColors
            | Self::Properties => selection.is_some_and(|s| {
                s.location == Location::Input && model::is_model(&s.path) && s.path.is_file()
            }),
            Self::Modifiers => selection.is_some_and(|s| {
                s.location == Location::Input
                    && (build::is_noise(&s.path) || build::is_image(&s.path))
                    && s.path.is_file()
            }),
            Self::Gradients => selection.is_some_and(|s| {
                s.location == Location::Input && gradient::is_gradient(&s.path) && s.path.is_file()
            }),
            Self::Colors => selection.is_some_and(|s| {
                s.location == Location::Input && palette::is_palette(&s.path) && s.path.is_file()
            }),
            Self::Graph => selection.is_some_and(|s| {
                s.location == Location::Input
                    && crate::agraph::is_graph(&s.path)
                    && s.path.is_file()
            }),
            Self::Atlas => selection.is_some_and(|s| {
                s.location == Location::Input && crate::atlas::is_atlas(&s.path) && s.path.is_dir()
            }),
            Self::Tree => selection.is_some_and(|s| {
                s.location == Location::Input && crate::atree::is_tree(&s.path) && s.path.is_file()
            }),
            Self::Instances => selection.is_some_and(|s| {
                s.location == Location::Input && crate::adata::is_data(&s.path) && s.path.is_file()
            }),
            Self::Types => selection.is_some_and(|s| {
                s.location == Location::Input
                    && crate::aschema::is_schema(&s.path)
                    && s.path.is_file()
            }),
            Self::Material => selection.is_some_and(|s| {
                s.location == Location::Input
                    && crate::material::is_source(&s.path)
                    && s.path.is_file()
            }),
            // See docs/features/structs.md#scopes.
            Self::Structs => selection.is_some_and(|s| {
                let p = &s.path;
                s.location == Location::Input
                    && ((p.is_file()
                        && (crate::agraph::is_graph(p)
                            || crate::atree::is_tree(p)
                            || model::is_model(p)
                            || crate::material::is_source(p)
                            || gradient::is_gradient(p)
                            || palette::is_palette(p)))
                        || (p.is_dir() && crate::atlas::is_atlas(p)))
            }),
            // Only compiled files are asset containers for now.
            Self::Assets => selection.is_some_and(|s| {
                s.location == Location::Output && s.path.is_file() && Kind::of(&s.path).is_some()
            }),
        }
    }
}

fn scan_trees(mut commands: Commands, project: Res<Project>) {
    commands.insert_resource(FileTrees::scan(&project));
    commands.insert_resource(Assets::load(&project));
}

#[allow(clippy::too_many_arguments)]
fn shell(
    mut contexts: EguiContexts,
    mut state: ResMut<UiState>,
    mut project: ResMut<Project>,
    mut trees: ResMut<FileTrees>,
    mut assets: ResMut<Assets>,
    mut exit: MessageWriter<AppExit>,
    mut logo: Local<Option<egui::TextureHandle>>,
    mut preview3d: ResMut<Preview3d>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let logo = logo.get_or_insert_with(|| {
        theme::apply(&ctx);
        egui_extras::install_image_loaders(&ctx);
        logo::load(&ctx)
    });

    let mut root = Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("menu_bar")
        .frame(bar_frame().inner_margin(Margin::symmetric(8, 0)))
        .show(&mut root, |ui| {
            menu_bar(ui, logo, &mut state, project.root.is_some(), &mut exit)
        });

    egui::Panel::bottom("status_bar")
        .frame(bar_frame())
        .show(&mut root, |ui| {
            let busy_since = [
                match &state.build {
                    BuildStatus::Running(_, since, _) => Some(*since),
                    _ => None,
                },
                state.inspector.running.as_ref().map(|(_, since, _)| *since),
                match preview3d.model_status {
                    ModelStatus::Loading(since) => Some(since),
                    _ => None,
                },
            ]
            .into_iter()
            .flatten()
            .min();
            status_bar(
                ui,
                &project,
                &state.build,
                busy_since,
                state.notice.as_deref(),
            )
        });

    egui::Panel::left("left_sidebar")
        .resizable(true)
        .default_size(260.0)
        .min_size(200.0)
        .frame(Frame::new().fill(BG_PANEL))
        .show(&mut root, |ui| {
            left_sidebar(ui, &mut state, &project, &trees)
        });

    egui::Panel::right("right_sidebar")
        .resizable(true)
        .default_size(320.0)
        .min_size(220.0)
        .frame(Frame::new().fill(BG_PANEL))
        .show(&mut root, |ui| {
            right_sidebar(
                ui,
                &mut state,
                project.input_dir(),
                &assets,
                preview3d.frames.as_ref(),
                preview3d.animation_time,
            )
        });

    // The chosen mip level, kept the same size on screen.
    let ui_state = &mut *state;
    if let Some(inspected) = ui_state.inspected.as_mut() {
        let level = ui_state
            .mip_levels
            .get(&inspected.path)
            .copied()
            .unwrap_or(0);
        if let Some(before) = show_level(&ctx, inspected, level)
            && let Some(after) = inspected.pixels.as_ref().map(|p| p.width)
            && !ui_state.view.fit
        {
            ui_state.view.zoom =
                (ui_state.view.zoom * before as f32 / after as f32).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }

    // In 3D the canvas is left transparent so the 3D camera's image shows through.
    preview3d.viewport = None;
    preview3d.dragging = false;
    preview3d.environment = false;
    preview3d.colors = false;
    preview3d.model = None;
    preview3d.compiled_shown = false;
    preview3d.attachments.clear();
    preview3d.material = None;
    preview3d.vision = state.vision;
    let shown = Shown::of(state.inspected.as_ref());
    let show_3d = match shown {
        Shown::Image => state.mode == ViewMode::ThreeD,
        Shown::Environment | Shown::Model | Shown::Material => true,
        Shown::CompiledMesh => state.mesh_in_3d,
        Shown::CompiledAnimation => {
            !state.anim_chunks
                && state
                    .inspected
                    .as_ref()
                    .is_some_and(|i| i.compiled_mesh.is_some())
        }
        Shown::Lut => state.lut.view == LutView::Cube,
        Shown::Specimen
        | Shown::CompiledFont
        | Shown::Pack
        | Shown::Data
        | Shown::Schema
        | Shown::Graph
        | Shown::Instances
        | Shown::CompiledData
        | Shown::Other => false,
    };
    let fill = if show_3d {
        egui::Color32::TRANSPARENT
    } else {
        BG_CENTER
    };
    egui::CentralPanel::default()
        .frame(Frame::new().fill(fill))
        .show(&mut root, |ui| {
            let state = &mut *state;
            claim_keys(ui, &mut state.keys, KeyArea::Canvas);
            let full = ui.max_rect();
            let bar = Rect::from_min_size(full.min, vec2(full.width(), 32.0));
            let modes = ViewMode::ALL.map(ViewMode::name);
            let mode = ViewMode::ALL.iter().position(|m| *m == state.mode);
            let lut_views = LutView::ALL.map(LutView::name);
            let pack_views = PackView::ALL.map(PackView::name);
            let pack_view_at = PackView::ALL.iter().position(|v| *v == state.pack_view);
            let lut_view = LutView::ALL.iter().position(|v| *v == state.lut.view);
            // Only images have a 2D/3D choice; an environment map only exists in 3D, a
            // specimen in 2D.
            let (options, active, enabled): (&[&str], _, _) = match shown {
                Shown::Image => (&modes, mode, true),
                Shown::Lut => (&lut_views, lut_view, true),
                Shown::Pack => (&pack_views, pack_view_at, true),
                Shown::CompiledMesh => (
                    &["3D", "Chunks"],
                    Some(usize::from(!state.mesh_in_3d)),
                    true,
                ),
                Shown::CompiledFont => (
                    &["Atlas", "Chunks"],
                    Some(usize::from(!state.font_atlas)),
                    true,
                ),
                Shown::Environment
                | Shown::Material
                | Shown::Specimen
                | Shown::Model
                | Shown::Data
                | Shown::Schema
                | Shown::Graph
                | Shown::Instances
                | Shown::CompiledData => (&[], None, false),
                Shown::CompiledAnimation => (
                    &["Play", "Chunks"],
                    Some(usize::from(state.anim_chunks)),
                    true,
                ),
                Shown::Other => (&modes, None, false),
            };
            if let Some(i) = view_bar(ui, bar, options, active, enabled) {
                match shown {
                    Shown::Lut => state.lut.view = LutView::ALL[i],
                    Shown::Pack => state.pack_view = PackView::ALL[i],
                    Shown::CompiledMesh => state.mesh_in_3d = i == 0,
                    Shown::CompiledFont => state.font_atlas = i == 0,
                    Shown::CompiledAnimation => state.anim_chunks = i == 1,
                    _ => state.mode = ViewMode::ALL[i],
                }
            }
            let rect = Rect::from_min_max(pos2(full.left(), bar.bottom()), full.max);
            if state.inspected.is_none() && state.selection.is_some() {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Loading…",
                    FontId::proportional(14.0),
                    TEXT_WEAK,
                );
                return;
            }
            match (shown, state.mode) {
                (Shown::Pack, _) if state.pack_view != PackView::Entries => {
                    let clicked = state.inspected.as_ref().and_then(|i| {
                        let contents = i.inspection.pack.as_ref()?;
                        pack_view(ui, rect, contents, &i.inspection.name, state.pack_view)
                    });
                    if let Some(entry) = clicked
                        && let Some(output) = project.output_dir()
                    {
                        reveal(state, &output, &output.join(entry));
                    }
                }
                (Shown::Material, _) => {
                    if let Some(inspected) = state.inspected.as_mut()
                        && let Some(light) = canvas_material(
                            ui,
                            rect,
                            inspected,
                            &mut preview3d,
                            &mut state.vision,
                            &state.material_light,
                        )
                    {
                        state.material_light = light;
                        state.reinspect = true;
                    }
                }
                (Shown::Environment, _) => canvas_environment(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut preview3d,
                    &mut state.vision,
                ),
                (Shown::CompiledMesh, _) | (Shown::CompiledFont, _)
                    if shown == Shown::CompiledMesh && !state.mesh_in_3d
                        || shown == Shown::CompiledFont && !state.font_atlas =>
                {
                    if let Some(bytes) = state
                        .inspected
                        .as_ref()
                        .and_then(|i| i.compiled_bytes.as_deref())
                    {
                        chunks_view(ui, rect, bytes, &mut state.mesh_chunk);
                    }
                }
                (Shown::CompiledData, _) | (Shown::CompiledAnimation, _)
                    if shown == Shown::CompiledData || state.anim_chunks =>
                {
                    if let Some(bytes) = state
                        .inspected
                        .as_ref()
                        .and_then(|i| i.compiled_bytes.as_deref())
                    {
                        chunks_view(ui, rect, bytes, &mut state.mesh_chunk);
                    }
                }
                (Shown::CompiledAnimation, _) => {
                    if let Some(inspected) = state.inspected.as_mut() {
                        canvas_compiled_animation(
                            ui,
                            rect,
                            inspected,
                            &mut preview3d,
                            &mut state.mesh_view,
                            &mut state.vision,
                        );
                    }
                }
                (Shown::CompiledMesh, _) => {
                    let path = state.inspected.as_ref().map(|i| &i.path);
                    let lod = path
                        .and_then(|p| state.lod_levels.get(p).copied())
                        .unwrap_or(0);
                    let color_set = path
                        .and_then(|p| state.mesh_colors.get(p).copied())
                        .flatten();
                    canvas_compiled_mesh(
                        ui,
                        rect,
                        state.inspected.as_mut(),
                        &mut preview3d,
                        &mut state.mesh_view,
                        (lod, color_set),
                        &mut state.vision,
                    )
                }
                (Shown::Model, _) => {
                    let inspected = state.inspected.as_ref();
                    let animation = inspected
                        .and_then(|i| state.animations.get(&i.path).copied())
                        .flatten();
                    preview3d.animation = animation;
                    // The timeline under the 3D view; see
                    // docs/features/animation-events.md#timeline.
                    let has_animations = inspected
                        .is_some_and(|i| i.model.is_some() && !i.inspection.animations.is_empty());
                    let mut rect = rect;
                    if state.inspector_tab == InspectorTab::Animations && has_animations {
                        let split = rect.bottom() - TIMELINE_HEIGHT;
                        let below = Rect::from_min_max(pos2(rect.left(), split), rect.max);
                        rect.max.y = split;
                        if let Some(inspected) = state.inspected.as_mut()
                            && let Some(amod) = timeline(
                                ui,
                                below,
                                inspected,
                                &mut preview3d,
                                &mut state.timeline,
                                animation,
                            )
                        {
                            match amod::write(&inspected.path, &amod) {
                                Ok(()) => {
                                    state.reinspect = true;
                                    state.rescan = true;
                                    state.keep_model = true;
                                }
                                Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                            }
                        }
                    }
                    preview3d.animation_paused = state.timeline.paused;
                    let out = canvas_model(
                        ui,
                        rect,
                        state.inspected.as_mut(),
                        &mut preview3d,
                        &mut state.vision,
                        &mut state.locators,
                        state.inspector_tab,
                    );
                    if let Some(tab) = out.show_tab {
                        state.inspector_tab = tab;
                    }
                    if let (Some(amod), Some(inspected)) = (out.save, &state.inspected) {
                        match amod::write(&inspected.path, &amod) {
                            Ok(()) => {
                                state.reinspect = true;
                                state.rescan = true;
                                state.keep_model = true;
                            }
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (Shown::Lut, _) => canvas_lut(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut state.lut,
                    state.last_image.as_ref(),
                    &mut preview3d,
                    &mut state.vision,
                ),
                (Shown::Schema, _) => {
                    if let Some(inspected) = state.inspected.as_mut()
                        && let Some(schema) =
                            canvas_schema(ui, rect, inspected, &mut state.schema_types)
                    {
                        match std::fs::write(&inspected.path, schema.to_json()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (Shown::Instances, _) => {
                    if let Some(inspected) = state.inspected.as_mut() {
                        if let Some(text) =
                            canvas_data(ui, rect, inspected, &mut state.data_instances)
                        {
                            match std::fs::write(&inspected.path, text) {
                                Ok(()) => state.reinspect = true,
                                Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                            }
                        }
                        // A ref's Go to: its target's file, with the target chosen.
                        let go = match &mut inspected.data {
                            Some(Ok(edit)) => edit.go_to.take(),
                            _ => None,
                        };
                        if let (Some(t), Some(input)) = (go, project.input_dir()) {
                            let path = input.join(&t.file);
                            state.data_instances.insert(path.clone(), t.index);
                            select(state, Location::Input, path);
                        }
                    }
                }
                (Shown::Graph, _) => {
                    if let Some(inspected) = state.inspected.as_mut()
                        && let Some(graph) = canvas_graph(
                            ui,
                            rect,
                            inspected,
                            &mut state.graph_views,
                            state.keys == KeyArea::Canvas,
                        )
                    {
                        match std::fs::write(&inspected.path, graph) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (Shown::Data | Shown::Pack, _) => {
                    if let Some(table) = state
                        .inspected
                        .as_ref()
                        .and_then(|i| i.inspection.data.as_ref())
                    {
                        data_view(ui, rect, table, &mut state.data_filter);
                    }
                }
                (Shown::Specimen | Shown::CompiledFont, _) | (_, ViewMode::TwoD) => canvas(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut state.view,
                    &mut state.canvas_options,
                    &mut state.vision,
                ),
                (_, ViewMode::ThreeD) => canvas_3d(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut state.canvas_options,
                    &mut preview3d,
                    &mut state.vision,
                ),
            }
        });

    confirm_delete(&ctx, &mut state, &project);
    if state.licenses {
        state.licenses = licenses_dialog(&ctx);
    }

    if std::mem::take(&mut state.open_config) && project.root.is_some() {
        state.config = Some(ConfigDialog::open(&project));
    }
    if let Some(dialog) = &mut state.config {
        match config_dialog(&ctx, dialog) {
            DialogEnd::Open => {}
            DialogEnd::Close => state.config = None,
            DialogEnd::Saved(loaded) => {
                state.config = None;
                *project = *loaded;
                state.build_marks = BuildMarks::default();
                state.notice = Some(format!("Saved {}", crate::config::FILE_NAME));
                state.rescan = true;
            }
        }
    }

    if std::mem::take(&mut state.start_build) {
        state.notice = None;
        let pack = std::mem::take(&mut state.pack);
        start_build(&mut state, &project, pack);
    }
    poll_build(&mut state);

    if std::mem::take(&mut state.rescan) {
        *trees = FileTrees::scan(&project);
        *assets = Assets::load(&project);
        state.reinspect = true;
    }
    if std::mem::take(&mut state.reinspect) {
        request_inspection(&ctx, &mut state, &project);
    }
    poll_inspection(&ctx, &mut state, &project);

    Ok(())
}

/// Background inspection, latest wins; see docs/features/inspector.md#loading.
#[derive(Default)]
struct Inspector {
    /// The inspection being made, and when it started.
    running: Option<(Selection, Instant, JoinHandle<Prepared>)>,
    /// The newest request made while one was running.
    waiting: Option<Selection>,
}

/// A model's sidecar, project mesh settings and inherited locators, see [`Prepared::mesh`].
type PreparedMesh = (Amod, MeshOptions, Vec<Inherited>, HeldLocators);

/// The part of an [`Inspected`] made off the UI thread.
struct Prepared {
    inspection: Inspection,
    pixels: Option<Image>,
    cubemap: Option<Vec<Image>>,
    mips: Vec<Image>,
    lut: Option<Lut>,
    noise: Option<Result<Noise, String>>,
    gradient: Option<Result<Gradients, String>>,
    palette: Option<Result<Palette, String>>,
    schema: Option<Result<Schema, (String, String)>>,
    /// A graph or (true) a tree.
    graph: Option<Result<(Graph, bool), String>>,
    data: Option<Result<PreparedData, (String, String)>>,
    atlas: Option<Result<crate::atlas::AtlasConfig, String>>,
    /// The input's files asset references can name, for a schema or data file.
    assets: Vec<String>,
    /// The instances refs can name: of the data files using a schema, or of a data file's
    /// project's other files; see docs/features/instances.md#references.
    refs: Vec<crate::adata::Target>,
    /// The file, input-relative with forward slashes.
    rel: String,
    /// A model's sidecar (with every locator the model has), the meshlet limits the project
    /// gives it, and the locators it inherits.
    mesh: Option<Result<PreparedMesh, String>>,
    sidecar: Option<Result<Amod, String>>,
    model: Option<(PathBuf, u64)>,
    geometry: Option<model::ModelGeometry>,
    compiled_mesh: Option<crate::amesh::Preview>,
    compiled_materials: Vec<Option<inspect::MaterialPreview>>,
    compiled_bytes: Option<Vec<u8>>,
    material: Option<Result<PreparedMaterial, String>>,
}

fn request_inspection(ctx: &egui::Context, state: &mut UiState, project: &Project) {
    let Some(selection) = state.selection.clone() else {
        state.inspector.waiting = None;
        state.inspected = None;
        return;
    };
    // A different file: never show (or edit) the previous file's result for it.
    if state
        .inspected
        .as_ref()
        .is_some_and(|i| i.path != selection.path)
    {
        state.inspected = None;
    }
    if state.inspector.running.is_some() {
        state.inspector.waiting = Some(selection);
    } else {
        let light = state.material_light.clone();
        start_inspection(ctx, &mut state.inspector, project, selection, light);
    }
}

/// A data file, its schema and the schemas it could name; see
/// docs/features/instances.md#schema.
fn prepare_data(project: &Project, file: &Path) -> Result<PreparedData, (String, String)> {
    use crate::adata::{DataFile, load_schema, relative, schemas_in};
    let text = std::fs::read_to_string(file).map_err(|e| (e.to_string(), String::new()))?;
    let data = DataFile::parse(&text).map_err(|e| (e, text.clone()))?;
    let (_, schema) = load_schema(file, &data);
    let dir = file.parent().unwrap_or(Path::new(""));
    let schemas = project
        .input_dir()
        .map(|input| {
            schemas_in(&input)
                .iter()
                .map(|p| relative(dir, p))
                .collect()
        })
        .unwrap_or_default();
    Ok((data, schema, schemas, text))
}

/// Inspects `selection` on a thread; `light` lights a material's preview.
fn start_inspection(
    ctx: &egui::Context,
    inspector: &mut Inspector,
    project: &Project,
    selection: Selection,
    light: Option<String>,
) {
    let max_side = ctx.input(|i| i.max_texture_side) as u32;
    let (project, job) = (project.clone(), selection.clone());
    let handle = std::thread::spawn(move || prepare(&project, &job, max_side, light.as_deref()));
    inspector.running = Some((selection, Instant::now(), handle));
}

fn prepare(
    project: &Project,
    selection: &Selection,
    max_side: u32,
    light: Option<&str>,
) -> Prepared {
    let mut inspection = inspect::inspect(project, selection.location, &selection.path);
    // See docs/features/materials.md#material-tab.
    let (material, light_faces) = match project.input_dir() {
        Some(input)
            if selection.location == Location::Input
                && crate::material::is_source(&selection.path) =>
        {
            let (material, faces) = material_edit::prepare(&input, &selection.path, light);
            (Some(material), faces)
        }
        _ => (None, None),
    };
    let noise =
        (selection.location == Location::Input && build::is_noise(&selection.path)).then(|| {
            std::fs::read_to_string(&selection.path)
                .map_err(|e| e.to_string())
                .and_then(|text| Noise::parse(&text))
        });
    // Each inspection of a model is a new generation, so the 3D view reads the file again.
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    let model = project
        .input_dir()
        .filter(|_| inspection.model && selection.location == Location::Input)
        .and_then(|input| {
            selection
                .path
                .strip_prefix(input)
                .ok()
                .map(Path::to_path_buf)
        })
        .map(|path| (path, GENERATION.fetch_add(1, Ordering::Relaxed)));
    let gradient = (selection.location == Location::Input
        && gradient::is_gradient(&selection.path))
    .then(|| {
        std::fs::read_to_string(&selection.path)
            .map_err(|e| e.to_string())
            .and_then(|text| Gradients::parse(&text))
    });
    let palette = (selection.location == Location::Input && palette::is_palette(&selection.path))
        .then(|| {
            std::fs::read_to_string(&selection.path)
                .map_err(|e| e.to_string())
                .and_then(|text| Palette::parse(&text))
        });
    let schema = (selection.location == Location::Input
        && crate::aschema::is_schema(&selection.path))
    .then(|| match std::fs::read_to_string(&selection.path) {
        Ok(text) => Schema::parse(&text).map_err(|e| (e, text)),
        Err(e) => Err((e.to_string(), String::new())),
    });
    let data = (selection.location == Location::Input && crate::adata::is_data(&selection.path))
        .then(|| prepare_data(project, &selection.path));
    // Directed graphs, and trees held as graphs; see docs/features/trees.md.
    let tree = crate::atree::is_tree(&selection.path);
    let graph = (selection.location == Location::Input
        && (crate::agraph::is_graph(&selection.path) || tree))
        .then(|| {
            let text = std::fs::read_to_string(&selection.path).map_err(|e| e.to_string())?;
            let graph = if tree {
                crate::atree::parse(&text)
            } else {
                Graph::parse(&text)
            };
            graph.map(|g| (g, tree))
        });
    let mesh =
        (selection.location == Location::Input && model::is_model(&selection.path)).then(|| {
            let settings = MeshSettings::new(&project.config.meshes)
                .map_err(|e| format!("assetc.toml [meshes]: {e}"))?;
            let rel = project
                .input_dir()
                .and_then(|dir| build::relative(&dir, &selection.path).ok())
                .unwrap_or_default();
            let mut amod = amod::read(&selection.path)?.unwrap_or_default();
            // See docs/features/locators.md#inherited-locators.
            let inherited = match project.input_dir() {
                Some(input) => {
                    let data = std::fs::read(&selection.path).map_err(|e| e.to_string())?;
                    presets::inherited(&project.config.locators, &input, &rel, &data)?
                }
                None => Vec::new(),
            };
            amod.locators = presets::merged(&inherited, &amod.locators);
            let held = HeldLocators {
                input: project.input_dir(),
                presets: project.config.locators.clone(),
                models: HashMap::new(),
            };
            Ok((amod, settings.options(&rel), inherited, held))
        });
    // Images carry modifiers in their sidecar; see docs/features/textures.md#modifiers.
    let sidecar = (selection.location == Location::Input && build::is_image(&selection.path))
        .then(|| Ok(amod::read(&selection.path)?.unwrap_or_default()));
    let geometry = (selection.location == Location::Input && model::is_model(&selection.path))
        .then(|| model::geometry(&selection.path).ok())
        .flatten();
    // The input's files, for asset references; see docs/aschema.md#asset-references.
    let assets = match (&schema, &data) {
        (None, None) => Vec::new(),
        _ => project
            .input_dir()
            .map(|input| crate::adata::asset_files(&input))
            .unwrap_or_default(),
    };
    let refs = match (&schema, &data, project.input_dir()) {
        (Some(Ok(_)), _, Some(input)) => crate::adata::project(&input, &selection.path),
        (_, Some(Ok((d, ..))), Some(input)) => crate::adata::others(&input, &selection.path, d),
        _ => Vec::new(),
    };
    let rel = project
        .input_dir()
        .and_then(|input| {
            let rel = selection.path.strip_prefix(input).ok()?;
            Some(rel.to_string_lossy().replace('\\', "/"))
        })
        .unwrap_or_default();
    Prepared {
        refs,
        rel,
        sidecar,
        geometry,
        compiled_mesh: inspection.compiled_mesh.take(),
        compiled_materials: std::mem::take(&mut inspection.compiled_materials),
        compiled_bytes: inspection.compiled_bytes.take(),
        gradient,
        palette,
        schema,
        graph,
        data,
        atlas: (selection.location == Location::Input && crate::atlas::is_atlas(&selection.path))
            .then(|| crate::atlas::AtlasConfig::read(&selection.path)),
        assets,
        mesh,
        model,
        pixels: inspection
            .preview
            .take()
            .map(|image| inspect::fit_within(image, max_side)),
        cubemap: inspection.cubemap.take().or(light_faces),
        material,
        mips: std::mem::take(&mut inspection.mips)
            .into_iter()
            .map(|image| inspect::fit_within(image, max_side))
            .collect(),
        lut: inspection.lut.take(),
        noise,
        inspection,
    }
}

fn poll_inspection(ctx: &egui::Context, state: &mut UiState, project: &Project) {
    let Some((_, _, handle)) = &state.inspector.running else {
        return;
    };
    if !handle.is_finished() {
        ctx.request_repaint();
        return;
    }
    let Some((selection, _, handle)) = state.inspector.running.take() else {
        return;
    };
    let prepared = handle.join();
    // A newer request supersedes this result.
    if let Some(next) = state.inspector.waiting.take() {
        let light = state.material_light.clone();
        start_inspection(ctx, &mut state.inspector, project, next, light);
        return;
    }
    if state.selection.as_ref() != Some(&selection) {
        return;
    }
    let Ok(prepared) = prepared else {
        state.notice = Some("Inspecting failed: inspector thread panicked".into());
        state.inspected = None;
        return;
    };
    let previous = state.inspected.take().filter(|i| i.path == selection.path);
    // A saved sidecar leaves the model loaded; see docs/features/locators.md#saving.
    let loaded = previous
        .as_ref()
        .and_then(|p| p.model.clone())
        .filter(|_| std::mem::take(&mut state.keep_model));
    let mut fresh = inspected(ctx, &selection, prepared, previous);
    if let Some(loaded) = loaded
        && fresh.model.as_ref().is_some_and(|m| m.0 == loaded.0)
    {
        fresh.model = Some(loaded);
    }
    if let Some(&level) = state.mip_levels.get(&fresh.path) {
        show_level(ctx, &mut fresh, level);
    }
    state.inspected = Some(fresh);
    // Remembered as the LUT compare view's "Image" source.
    if let Some(inspected) = &state.inspected
        && let Some(pixels) = &inspected.pixels
        && !inspected.inspection.specimen
    {
        state.last_image = Some(LastImage {
            path: inspected.path.clone(),
            name: inspected.inspection.name.clone(),
            image: pixels.clone(),
        });
    }
    ctx.request_repaint();
}

/// Uploads a finished inspection. `previous` is the current result for the same file, whose
/// noise edit is kept when the file still holds what it saved (so edits in progress survive).
fn inspected(
    ctx: &egui::Context,
    selection: &Selection,
    prepared: Prepared,
    previous: Option<Inspected>,
) -> Inspected {
    let mut previous = previous;
    let kept_data = previous
        .as_mut()
        .and_then(|p| p.data.take())
        .and_then(Result::ok);
    let kept_material = previous
        .as_mut()
        .and_then(|p| p.material.take())
        .and_then(Result::ok);
    let material = prepared
        .material
        .map(|parsed| parsed.map(|p| MaterialEdit::new(p, kept_material)));
    let data = prepared.data.map(|parsed| {
        parsed.map(|p| {
            let mut edit = DataEdit::new(p, kept_data);
            edit.assets = prepared.assets.clone();
            edit.others = prepared.refs.clone();
            edit.file = prepared.rel.clone();
            edit
        })
    });
    let (kept, kept_gradient, kept_palette, kept_mesh, kept_sidecar, kept_schema, kept_graph) =
        previous.map_or((None, None, None, None, None, None, None), |p| {
            (
                p.noise.and_then(Result::ok),
                p.gradient.and_then(Result::ok),
                p.palette.and_then(Result::ok),
                p.mesh.and_then(Result::ok),
                p.sidecar.and_then(Result::ok),
                p.schema.and_then(Result::ok),
                p.graph.and_then(Result::ok),
            )
        });
    let graph = prepared.graph.map(|parsed| {
        parsed.map(|(graph, tree)| match kept_graph {
            Some(edit) if edit.saved == graph && edit.tree == tree => edit,
            Some(edit) => GraphEdit::new(graph, tree, edit),
            None => GraphEdit::new(graph, tree, GraphEdit::default()),
        })
    });
    let schema = prepared.schema.map(|parsed| {
        parsed.map(|schema| match kept_schema {
            Some(edit) if edit.saved == schema => edit,
            Some(edit) => SchemaEdit::new(schema, edit),
            None => SchemaEdit::new(schema, SchemaEdit::default()),
        })
    });
    let schema = schema.map(|parsed| {
        parsed.map(|mut edit| {
            edit.assets = prepared.assets.clone();
            edit.refs = prepared.refs.clone();
            edit
        })
    });
    let palette = prepared.palette.map(|parsed| {
        parsed.map(|p| match kept_palette {
            Some(edit) if edit.saved == p => edit,
            Some(edit) => PaletteEdit {
                edit: p.clone(),
                saved: p,
                ..edit
            },
            None => PaletteEdit {
                edit: p.clone(),
                saved: p,
                selected: 0,
                drag: None,
                picker: Picker::default(),
            },
        })
    });
    let sidecar = prepared.sidecar.map(|parsed| {
        parsed.map(|amod| match kept_sidecar {
            Some(edit) if edit.saved == amod => edit,
            _ => SidecarEdit {
                edit: amod.clone(),
                saved: amod,
            },
        })
    });
    let mesh = prepared.mesh.map(|parsed| {
        parsed.map(|(amod, project, inherited, held)| match kept_mesh {
            Some(edit) if edit.saved == amod => MeshEdit {
                project,
                inherited,
                held,
                ..edit
            },
            _ => MeshEdit {
                edit: amod.clone(),
                saved: amod,
                project,
                locator: None,
                shape: None,
                region: None,
                path: None,
                point: None,
                event: None,
                color: None,
                color_end: 0,
                color_cards: Vec::new(),
                inherited,
                held,
            },
        })
    });
    let gradient = prepared.gradient.map(|parsed| {
        parsed.map(|g| match kept_gradient {
            Some(edit) if edit.saved == g => edit,
            _ => GradientEdit {
                edit: g.clone(),
                saved: g,
                cards: Vec::new(),
            },
        })
    });
    let noise = prepared.noise.map(|parsed| {
        parsed.map(|n| match kept {
            Some(edit) if edit.saved == n => edit,
            _ => NoiseEdit {
                edit: n.clone(),
                saved: n,
            },
        })
    });
    let preview = prepared
        .pixels
        .as_ref()
        .map(|image| load_preview(ctx, "canvas_preview", image, &image.pixels));
    let levels = match &prepared.pixels {
        Some(level0) if !prepared.mips.is_empty() => std::iter::once(level0.clone())
            .chain(prepared.mips)
            .collect(),
        _ => Vec::new(),
    };
    Inspected {
        path: selection.path.clone(),
        inspection: prepared.inspection,
        pixels: prepared.pixels,
        levels,
        shown_level: 0,
        preview,
        channel_preview: None,
        vision_preview: None,
        uploaded_3d: None,
        cubemap: prepared.cubemap,
        uploaded_cubemap: false,
        lut: prepared.lut,
        noise,
        gradient,
        palette,
        schema,
        graph,
        data,
        mesh,
        sidecar,
        material,
        uploaded_material: false,
        compiled_mesh: prepared.compiled_mesh,
        compiled_materials: prepared.compiled_materials,
        uploaded_lod: None,
        anim_player: AnimPlayer::default(),
        atlas: prepared.atlas.map(|r| r.map(|c| (c.clone(), c))),
        compiled_bytes: prepared.compiled_bytes,
        geometry: prepared.geometry,
        hulls: HashMap::new(),
        structs_cache: None,
        lut_compare: None,
        uploaded_color_cube: false,
        model: prepared.model,
    }
}

/// Makes the canvas show mip `level` (clamped) of a compiled texture; returns the width of the
/// level shown before, if it changed. See docs/features/inspector.md#mipmaps-tab.
fn show_level(ctx: &egui::Context, inspected: &mut Inspected, level: usize) -> Option<u32> {
    let level = level.min(inspected.levels.len().checked_sub(1)?);
    if level == inspected.shown_level {
        return None;
    }
    let before = inspected.levels[inspected.shown_level].width;
    let image = inspected.levels[level].clone();
    inspected.preview = Some(load_preview(ctx, "canvas_preview", &image, &image.pixels));
    inspected.pixels = Some(image);
    inspected.channel_preview = None;
    inspected.vision_preview = None;
    inspected.uploaded_3d = None;
    inspected.shown_level = level;
    Some(before)
}

/// Uploads RGBA8 `pixels` of `image`'s size with the canvas filtering (sharp when magnified).
fn load_preview(
    ctx: &egui::Context,
    name: &str,
    image: &Image,
    pixels: &[u8],
) -> egui::TextureHandle {
    load_texture(ctx, name, image, pixels, egui::TextureFilter::Nearest)
}

/// Uploads RGBA8 `pixels` of `image`'s size, smoothed when minified.
fn load_texture(
    ctx: &egui::Context,
    name: &str,
    image: &Image,
    pixels: &[u8],
    magnification: egui::TextureFilter,
) -> egui::TextureHandle {
    let size = [image.width as usize, image.height as usize];
    let options = egui::TextureOptions {
        magnification,
        minification: egui::TextureFilter::Linear,
        ..Default::default()
    };
    let image = egui::ColorImage::from_rgba_unmultiplied(size, pixels);
    ctx.load_texture(name, image, options)
}

/// Builds, then packs when `pack`, on a thread; see docs/features/build.md#gui and
/// docs/features/pack.md#gui.
fn start_build(state: &mut UiState, project: &Project, pack: bool) {
    if matches!(state.build, BuildStatus::Running(..)) {
        return;
    }
    let (Some(input), Some(output)) = (project.input_dir(), project.output_dir()) else {
        return;
    };
    let config = project.config.clone();
    let packing = Arc::new(AtomicBool::new(false));
    let flag = packing.clone();
    let run = move || {
        let settings = match pack.then(|| crate::pack::Settings::new(&config.pack)) {
            Some(Err(e)) => return (format!("Pack failed: {e}"), BuildMarks::default()),
            Some(Ok(settings)) => Some(settings),
            None => None,
        };
        let outcome = match build::run(&input, &output, &config) {
            Ok(outcome) => outcome,
            Err(e) => return (format!("Build failed: {e}"), BuildMarks::default()),
        };
        let marks = BuildMarks::new(&input, &outcome);
        let Some(settings) = settings.filter(|_| outcome.failures.is_empty()) else {
            return (outcome.summary(), marks);
        };
        flag.store(true, Ordering::Relaxed);
        let message = match crate::pack::write(&output, &settings) {
            Ok((_, contents)) => crate::pack::summary(&contents, &settings.file),
            Err(e) => format!("Pack failed: {e}"),
        };
        (message, marks)
    };
    state.build = BuildStatus::Running(std::thread::spawn(run), Instant::now(), packing);
}

fn poll_build(state: &mut UiState) {
    let BuildStatus::Running(handle, ..) = &state.build else {
        return;
    };
    if !handle.is_finished() {
        return;
    }
    let BuildStatus::Running(handle, ..) = std::mem::replace(&mut state.build, BuildStatus::Idle)
    else {
        unreachable!();
    };
    let (message, marks) = handle.join().unwrap_or_else(|_| {
        let message = "Build failed: build thread panicked".to_owned();
        (message, BuildMarks::default())
    });
    state.build = BuildStatus::Finished(message);
    state.build_marks = marks;
    state.rescan = true;
}

fn bar_frame() -> Frame {
    Frame::new()
        .fill(BG_BAR)
        .inner_margin(Margin::symmetric(8, 4))
        .stroke(Stroke::new(1.0, STROKE))
}

const BUILD_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::B);
const RESCAN_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::R);
const PACK_SHORTCUT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(
    egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT),
    egui::Key::B,
);
const CONFIG_SHORTCUT: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Comma);

fn menu_bar(
    ui: &mut Ui,
    logo: &egui::TextureHandle,
    state: &mut UiState,
    has_project: bool,
    exit: &mut MessageWriter<AppExit>,
) {
    // Square, gap-free items as tall as the bar, so the highlight covers the whole item.
    let style = |style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        style.spacing.button_padding = vec2(12.0, 7.0);
        style.spacing.item_spacing.x = 0.0;
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.0));
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.corner_radius = 0.into();
            widget.expansion = 0.0;
        }
    };
    egui::MenuBar::new().style(style).ui(ui, |ui| {
        // Same height as the menu buttons so the logo is centered with their text.
        let size = logo.size_vec2() * (logo::HEIGHT / logo.size_vec2().y);
        let row_height =
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y;
        let (rect, _) = ui.allocate_exact_size(vec2(size.x, row_height), Sense::hover());
        egui::Image::new((logo.id(), size))
            .paint_at(ui, Rect::from_center_size(rect.center(), size));
        ui.add_space(4.0);
        let quit = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Q);
        if ui.input_mut(|i| i.consume_shortcut(&quit)) {
            exit.write(AppExit::Success);
        }
        // Build, pack, config and rescan; see docs/features/build.md#gui,
        // docs/features/pack.md#gui, docs/features/config.md and docs/features/gui.md#file-browser.
        // Not while a dialog is open; pack first, as its shortcut holds the build's.
        let building = matches!(state.build, BuildStatus::Running(..));
        if state.config.is_none() {
            if ui.input_mut(|i| i.consume_shortcut(&PACK_SHORTCUT)) {
                state.start_build = true;
                state.pack = true;
            }
            if ui.input_mut(|i| i.consume_shortcut(&BUILD_SHORTCUT)) {
                state.start_build = true;
            }
            if ui.input_mut(|i| i.consume_shortcut(&CONFIG_SHORTCUT)) {
                state.open_config = true;
            }
            if ui.input_mut(|i| i.consume_shortcut(&RESCAN_SHORTCUT)) {
                state.rescan = true;
            }
        }
        ui.menu_button("File", |ui| {
            if shortcut_row(ui, "Quit", &quit, true).clicked() {
                exit.write(AppExit::Success);
            }
        });
        ui.menu_button("Build", |ui| {
            if shortcut_row(ui, "Config…", &CONFIG_SHORTCUT, has_project).clicked() {
                state.open_config = true;
            }
            let can_build = has_project && !building;
            if shortcut_row(ui, "Build", &BUILD_SHORTCUT, can_build).clicked() {
                state.start_build = true;
            }
            if shortcut_row(ui, "Pack", &PACK_SHORTCUT, can_build).clicked() {
                state.start_build = true;
                state.pack = true;
            }
        });
        ui.menu_button("Help", |ui| {
            menu_row(
                ui,
                &format!("assetc {}", env!("CARGO_PKG_VERSION")),
                None,
                false,
            );
            if menu_item(ui, "Licenses", None).clicked() {
                state.licenses = true;
            }
        });
    });
}

/// The Open source dialog; returns whether it stays open. See docs/features/gui.md#open-source.
fn licenses_dialog(ctx: &egui::Context) -> bool {
    let frame = Frame::window(&ctx.global_style())
        .fill(BG_PANEL)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(8)
        .inner_margin(24);
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::new("licenses"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.label(RichText::new("Open source").size(20.0).color(TEXT));
            ui.add_space(8.0);
            ui.label(
                RichText::new("assetc is built with these open source projects:")
                    .size(14.0)
                    .color(TEXT_WEAK),
            );
            ui.add_space(12.0);
            egui::Grid::new("licenses_grid")
                .spacing(vec2(24.0, 6.0))
                .show(ui, |ui| {
                    let text = |s: &str, color| RichText::new(s).size(14.0).color(color);
                    for p in &crate::licenses::PROJECTS {
                        ui.label(text(p.name, TEXT));
                        ui.label(text(p.role, TEXT_WEAK));
                        ui.label(text(p.license, TEXT_WEAK));
                        ui.hyperlink_to(text(p.site, ACCENT), format!("https://{}", p.site));
                        ui.end_row();
                    }
                });
            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let close = egui::Button::new(RichText::new("Close").size(14.0).color(TEXT))
                    .fill(BG_HEADER)
                    .stroke(Stroke::new(1.0, STROKE))
                    .corner_radius(6)
                    .min_size(vec2(96.0, 34.0));
                if ui.add(close).clicked() {
                    open = false;
                }
            });
        });
    open && !modal.should_close()
}

/// A dropdown row with its shortcut right-aligned; disabled rows are weak and not clickable.
fn shortcut_row(
    ui: &mut Ui,
    label: &str,
    shortcut: &egui::KeyboardShortcut,
    enabled: bool,
) -> Response {
    let response = menu_row(ui, label, None, enabled);
    ui.painter().text(
        pos2(response.rect.right() - 12.0, response.rect.center().y),
        Align2::RIGHT_CENTER,
        ui.ctx().format_shortcut(shortcut),
        FontId::proportional(14.0),
        TEXT_WEAK,
    );
    response
}

/// Full-width dropdown row; `check` is `Some` for toggles.
fn menu_item(ui: &mut Ui, label: &str, check: Option<bool>) -> Response {
    menu_row(ui, label, check, true)
}

fn menu_row(ui: &mut Ui, label: &str, check: Option<bool>, interactive: bool) -> Response {
    let sense = if interactive {
        Sense::click()
    } else {
        Sense::hover()
    };
    // Fixed width: popups offer far more room than their items need.
    let width = 180.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 28.0), sense);
    if interactive && response.hovered() {
        ui.painter().rect_filled(rect, 0.0, BG_HEADER);
    }
    if check == Some(true) {
        let at = pos2(rect.left() + 12.0, rect.center().y);
        Icon::Check
            .image(14.0, TEXT)
            .paint_at(ui, Rect::from_center_size(at, vec2(14.0, 14.0)));
    }
    ui.painter().text(
        pos2(rect.left() + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        if interactive { TEXT } else { TEXT_WEAK },
    );
    response
}

/// Background work shows the loader only after this long, so quick work does not flicker it.
const LOADER_DELAY: Duration = Duration::from_millis(150);

fn status_bar(
    ui: &mut Ui,
    project: &Project,
    build: &BuildStatus,
    busy_since: Option<Instant>,
    notice: Option<&str>,
) {
    ui.horizontal(|ui| {
        let busy = busy_since.is_some_and(|since| since.elapsed() >= LOADER_DELAY);
        if busy {
            // One turn per second.
            let angle = (ui.input(|i| i.time) % 1.0) as f32 * std::f32::consts::TAU;
            let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            Icon::Loader
                .image(14.0, ACCENT)
                .rotate(angle, Vec2::splat(0.5))
                .paint_at(ui, rect);
            ui.ctx().request_repaint();
        }
        match build {
            _ if let Some(notice) = notice => ui.label(notice),
            BuildStatus::Running(_, _, packing) if packing.load(Ordering::Relaxed) => {
                ui.label("Packing…")
            }
            BuildStatus::Running(..) => ui.label("Building…"),
            _ if busy => ui.label("Processing…"),
            BuildStatus::Finished(message) => ui.label(message),
            BuildStatus::Idle if project.root.is_some() => ui.label("Ready"),
            BuildStatus::Idle => ui.label("No assetc.toml found (run assetc init)"),
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("assetc {}", env!("CARGO_PKG_VERSION")));
        });
    });
}

fn left_sidebar(ui: &mut Ui, state: &mut UiState, project: &Project, trees: &FileTrees) {
    claim_keys(ui, &mut state.keys, KeyArea::Tree);
    let strip = tab_strip(ui, vec2(ui.available_width(), 32.0));
    let mut tabs = ui.new_child(
        UiBuilder::new()
            .max_rect(strip)
            .layout(Layout::left_to_right(Align::Center)),
    );
    tabs.spacing_mut().item_spacing.x = 0.0;
    for tab in BrowserTab::ALL {
        if text_tab(&mut tabs, tab.name(), state.browser_tab == tab).clicked() {
            state.browser_tab = tab;
        }
    }
    tabs.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add_space(6.0);
        let shortcut = |s: &egui::KeyboardShortcut| ui.ctx().format_shortcut(s);
        let (rescan, build) = (shortcut(&RESCAN_SHORTCUT), shortcut(&BUILD_SHORTCUT));
        if icon_button(ui, Icon::RefreshCw, &format!("Rescan ({rescan})"), true).clicked() {
            state.rescan = true;
        }
        let can_build = project.root.is_some() && !matches!(state.build, BuildStatus::Running(..));
        if icon_button(ui, Icon::Hammer, &format!("Build ({build})"), can_build).clicked() {
            state.start_build = true;
        }
    });

    let listing = match state.browser_tab {
        BrowserTab::Input => &trees.input,
        BrowserTab::Output => &trees.output,
    };
    let location = state.browser_tab.location();
    match listing {
        Listing::NoProject => placeholder(ui, "No project open"),
        Listing::Missing(dir) => placeholder(ui, &format!("{} does not exist", dir.display())),
        Listing::Failed(e) => placeholder(ui, e),
        Listing::Loaded(nodes) => {
            let editable = location == Location::Input;
            // Not while renaming or while a dialog is open.
            let keys_free = state.keys == KeyArea::Tree
                && state.rename.is_none()
                && state.confirm_delete.is_none()
                && state.config.is_none()
                && !state.licenses
                && !ui.ctx().egui_wants_keyboard_input();
            if keys_free {
                tree_keys(ui, state, nodes, location);
            }
            let scroll_to = std::mem::take(&mut state.tree_scroll)
                .then(|| state.selection.as_ref().map(|s| s.path.clone()))
                .flatten();
            let mut cx = TreeCx {
                location,
                selection: state.selection.as_ref(),
                open: &mut state.open,
                editable,
                rename: &mut state.rename,
                rename_done: None,
                action: None,
                scroll_to,
                marks: (location == Location::Input).then_some(&state.build_marks),
            };
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if nodes.is_empty() {
                        placeholder(ui, "Empty");
                    }
                    tree(ui, nodes, 0, &mut cx);
                    // The empty area below the rows stands for the input directory itself.
                    let rest = vec2(ui.available_width(), ui.available_height().max(24.0));
                    let (_, response) = ui.allocate_exact_size(rest, Sense::click());
                    if editable && let Some(input) = project.input_dir() {
                        response.context_menu(|ui| create_items(ui, &input, &mut cx.action));
                    }
                });
            let (action, rename_done) = (cx.action, cx.rename_done);
            match rename_done {
                Some(true) => commit_rename(state, project),
                Some(false) => state.rename = None,
                None => {}
            }
            match action {
                Some(TreeAction::Select(path)) => select(state, location, path),
                Some(TreeAction::Op(op)) => apply_file_op(state, project, op),
                None => {}
            }
            // F2 / Delete act on the selected Input entry, once the sidebar has the keys.
            let keys_free =
                editable && state.keys == KeyArea::Tree && !ui.ctx().egui_wants_keyboard_input();
            if let Some(selection) = state
                .selection
                .clone()
                .filter(|s| keys_free && s.location == location)
            {
                if ui.input(|i| i.key_pressed(egui::Key::F2)) {
                    apply_file_op(state, project, FileOp::Rename(selection.path));
                } else if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                    apply_file_op(state, project, FileOp::Delete(selection.path));
                }
            }
        }
    }
}

/// Selects `file` (under `root`) in the Output tree: switches to it, opens the folders above
/// it and scrolls to it; nothing when the file does not exist.
fn reveal(state: &mut UiState, root: &Path, file: &Path) {
    if !file.is_file() {
        return;
    }
    let mut dir = file.parent();
    while let Some(d) = dir.filter(|d| d.starts_with(root) && *d != root) {
        state.open.insert(d.to_path_buf());
        dir = d.parent();
    }
    state.browser_tab = BrowserTab::Output;
    select(state, Location::Output, file.to_path_buf());
    state.tree_scroll = true;
}

/// Moves the selection or folds a folder by key; see docs/features/gui.md#file-browser. Every
/// press counts, also several in one frame.
fn tree_keys(ui: &mut Ui, state: &mut UiState, nodes: &[Node], location: Location) {
    // The rows that fit in the view, minus one.
    let page = ((ui.available_height() / 22.0) as usize)
        .saturating_sub(1)
        .max(1);
    for key in tree_nav::KEYS {
        let presses = ui.input_mut(|i| i.count_and_consume_key(egui::Modifiers::NONE, key));
        for _ in 0..presses {
            let rows = tree_nav::visible_rows(nodes, &state.open);
            let selected = state
                .selection
                .as_ref()
                .filter(|s| s.location == location)
                .map(|s| s.path.as_path());
            match tree_nav::navigate(&rows, selected, key, page) {
                Some(tree_nav::Move::Select(path)) => {
                    select(state, location, path);
                    state.tree_scroll = true;
                }
                Some(tree_nav::Move::Fold(path, true)) => {
                    state.open.insert(path);
                }
                Some(tree_nav::Move::Fold(path, false)) => {
                    state.open.remove(&path);
                }
                None => {}
            }
        }
    }
}

/// Allocates a tab strip background with a separator line along its bottom edge.
fn tab_strip(ui: &mut Ui, size: egui::Vec2) -> Rect {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, BG_BAR);
    painter.hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, STROKE),
    );
    rect
}

fn tab_color(selected: bool, hovered: bool) -> egui::Color32 {
    if selected || hovered { TEXT } else { TEXT_WEAK }
}

/// Horizontal tab attached to the pane below: active tab takes the pane color.
fn text_tab(ui: &mut Ui, label: &str, selected: bool) -> Response {
    let font = FontId::proportional(14.0);
    let text_width = ui
        .painter()
        .layout_no_wrap(label.into(), font.clone(), TEXT)
        .size()
        .x;
    let size = vec2(text_width + 24.0, ui.available_height());
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 0.0, BG_PANEL);
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 2.0)),
            0.0,
            ACCENT,
        );
    }
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        font,
        tab_color(selected, response.hovered()),
    );
    response
}

/// Vertical icon tab attached to the pane on its right.
fn icon_tab(ui: &mut Ui, icon: Icon, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    if selected {
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, BG_PANEL);
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            ACCENT,
        );
    }
    icon.image(18.0, tab_color(selected, response.hovered()))
        .paint_at(ui, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)));
    response
}

fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), sense);
    let color = match (enabled, response.hovered()) {
        (false, _) => TEXT_WEAK.gamma_multiply(0.5),
        (true, true) => TEXT,
        (true, false) => TEXT_WEAK,
    };
    icon.image(16.0, color)
        .paint_at(ui, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)));
    response.on_hover_text(tooltip)
}

/// An icon button that is on (`ACCENT`) or off.
fn icon_toggle(ui: &mut Ui, icon: Icon, tooltip: &str, on: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let color = match (on, response.hovered()) {
        (true, _) => ACCENT,
        (false, true) => TEXT,
        (false, false) => TEXT_WEAK,
    };
    icon.image(16.0, color)
        .paint_at(ui, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)));
    response.on_hover_text(tooltip)
}

/// The grip a modifier card is dragged by, see docs/features/noise.md#modifiers-tab.
fn drag_handle(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::drag());
    paint_drag_handle(ui, rect, response)
}

/// [`drag_handle`] with its own `id`, for grips whose place in the Ui changes mid-drag.
fn drag_handle_with_id(ui: &mut Ui, id: egui::Id) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
    let response = ui.interact(rect, id, Sense::drag());
    paint_drag_handle(ui, rect, response)
}

fn paint_drag_handle(ui: &Ui, rect: Rect, response: Response) -> Response {
    let color = if response.hovered() || response.dragged() {
        TEXT
    } else {
        TEXT_WEAK
    };
    Icon::GripHorizontal
        .image(16.0, color)
        .paint_at(ui, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)));
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    response
}

fn select(state: &mut UiState, location: Location, path: PathBuf) {
    let selection = Some(Selection { location, path });
    if state.selection != selection {
        state.selection = selection;
        state.reinspect = true;
        state.view = View::default();
    }
}

/// A file operation requested from the Input tree, see docs/features/file-operations.md.
enum FileOp {
    NewFolder(PathBuf),
    NewNoise(PathBuf, GeneratorKind),
    NewGradient(PathBuf),
    NewPalette(PathBuf),
    NewSchema(PathBuf),
    NewGraph(PathBuf),
    NewData(PathBuf),
    NewTree(PathBuf),
    NewMaterial(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
}

enum TreeAction {
    Select(PathBuf),
    Op(FileOp),
}

/// An inline rename in the Input tree.
struct Rename {
    path: PathBuf,
    text: String,
    /// Focus and select the name on the first frame.
    started: bool,
}

/// What tree rows need while drawing; collects the action of this frame.
struct TreeCx<'a> {
    location: Location,
    selection: Option<&'a Selection>,
    open: &'a mut HashSet<PathBuf>,
    /// File operations are only offered in the Input tree.
    editable: bool,
    rename: &'a mut Option<Rename>,
    /// Set when a rename should be committed (`Some(true)`) or cancelled (`Some(false)`).
    rename_done: Option<bool>,
    action: Option<TreeAction>,
    /// The row to scroll into view.
    scroll_to: Option<PathBuf>,
    /// The last build's marks, in the Input tree only.
    marks: Option<&'a BuildMarks>,
}

/// Menu items creating something in `dir`.
fn create_items(ui: &mut Ui, dir: &Path, action: &mut Option<TreeAction>) {
    if menu_item(ui, "New Folder", None).clicked() {
        *action = Some(TreeAction::Op(FileOp::NewFolder(dir.to_path_buf())));
        ui.close();
    }
    // Same size, color and text inset as `menu_row`.
    ui.spacing_mut().button_padding = vec2(24.0, 7.0);
    for (title, group) in [("New Noise", "noise/"), ("New Pattern", "pattern/")] {
        ui.menu_button(RichText::new(title).size(14.0).color(TEXT), |ui| {
            for kind in GeneratorKind::ALL
                .into_iter()
                .filter(|k| k.key().starts_with(group))
            {
                if menu_item(ui, kind.name(), None).clicked() {
                    *action = Some(TreeAction::Op(FileOp::NewNoise(dir.to_path_buf(), kind)));
                    ui.close();
                }
            }
        });
    }
    ui.menu_button(RichText::new("New Color").size(14.0).color(TEXT), |ui| {
        if menu_item(ui, "Gradient", None).clicked() {
            *action = Some(TreeAction::Op(FileOp::NewGradient(dir.to_path_buf())));
            ui.close();
        }
        if menu_item(ui, "Palette", None).clicked() {
            *action = Some(TreeAction::Op(FileOp::NewPalette(dir.to_path_buf())));
            ui.close();
        }
    });
    if menu_item(ui, "New Material", None).clicked() {
        *action = Some(TreeAction::Op(FileOp::NewMaterial(dir.to_path_buf())));
        ui.close();
    }
    ui.menu_button(RichText::new("New Data").size(14.0).color(TEXT), |ui| {
        if menu_item(ui, "Schema", None).clicked() {
            *action = Some(TreeAction::Op(FileOp::NewSchema(dir.to_path_buf())));
            ui.close();
        }
        if menu_item(ui, "Directed Graph", None).clicked() {
            *action = Some(TreeAction::Op(FileOp::NewGraph(dir.to_path_buf())));
            ui.close();
        }
        if menu_item(ui, "Tree", None).clicked() {
            *action = Some(TreeAction::Op(FileOp::NewTree(dir.to_path_buf())));
            ui.close();
        }
        if menu_item(ui, "Instance", None).clicked() {
            *action = Some(TreeAction::Op(FileOp::NewData(dir.to_path_buf())));
            ui.close();
        }
    });
}

/// The row icon by type, see docs/features/gui.md#tree-icons.
/// An atlas folder's `atlas.toml`, dimmed like a sidecar; see docs/features/gui.md#file-browser.
fn is_atlas_config(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(crate::atlas::CONFIG))
        && path.parent().is_some_and(crate::atlas::is_atlas)
}

fn tree_icon(path: &Path, is_dir: bool, open: bool) -> Icon {
    if is_dir {
        if crate::atlas::is_atlas(path) {
            return Icon::Map;
        }
        return match (inspect::is_environment_map(path), open) {
            (true, _) => Icon::SunMedium,
            (false, true) => Icon::FolderOpen,
            (false, false) => Icon::Folder,
        };
    }
    if is_atlas_config(path) {
        return Icon::Settings2;
    }
    // A compiled `<name>.<format>.bin` takes the icon of its format.
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = name.strip_suffix(".bin").unwrap_or(&name);
    let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "tga" | "webp" | "tif" | "tiff" | "hdr"
        | "dds" | "ktx2" => Icon::Image,
        "ttf" | "otf" | "woff" | "woff2" => Icon::Type,
        "cube" => Icon::Aperture,
        "anoise" => Icon::ImagePlay,
        "agradient" => Icon::SwatchBook,
        "apalette" => Icon::Palette,
        "aschema" => Icon::Braces,
        "agraph" => Icon::GraphStar,
        "adata" | "pak" => Icon::Package,
        "atree" => Icon::ListTree,
        "aatlas" => Icon::Map,
        "amat" => Icon::Gem,
        "amod" => Icon::Settings2,
        "gltf" | "glb" | "obj" | "amesh" => Icon::Torus,
        "aanim" => Icon::Film,
        "exr" => Icon::SunMedium,
        "afont" => Icon::Type,
        _ => Icon::File,
    }
}

fn tree(ui: &mut Ui, nodes: &[Node], depth: usize, cx: &mut TreeCx) {
    // A file's `.amod` sidecar sits right below it; see docs/features/gui.md#file-browser.
    for node in nodes {
        if tree_nav::nested(nodes, node) {
            continue;
        }
        let is_dir = node.children.is_some();
        let open = cx.open.contains(&node.path);
        let sidecar = tree_nav::sidecar_of(nodes, node);
        let chevron = match (is_dir, open) {
            (true, true) => Some(Icon::ChevronDown),
            (true, false) => Some(Icon::ChevronRight),
            (false, _) => None,
        };
        tree_entry(ui, node, depth, chevron, cx);
        if let Some(sidecar) = sidecar {
            tree_entry(ui, sidecar, depth + 1, None, cx);
        }
        if let Some(children) = &node.children
            && open
        {
            tree(ui, children, depth + 1, cx);
        }
    }
}

/// One row of the tree with its clicks and context menu.
fn tree_entry(ui: &mut Ui, node: &Node, depth: usize, chevron: Option<Icon>, cx: &mut TreeCx) {
    let is_dir = node.children.is_some();
    let open = cx.open.contains(&node.path);
    let icon = tree_icon(&node.path, is_dir, open);
    // Sidecars are dimmed.
    let dim = !is_dir && (amod::is_sidecar(&node.path) || is_atlas_config(&node.path));
    let selected = cx
        .selection
        .is_some_and(|s| s.location == cx.location && s.path == node.path);
    let renaming = cx.rename.as_mut().filter(|r| r.path == node.path);
    let mark = cx.marks.and_then(|m| m.of(&node.path));
    let (response, done) = tree_row(
        ui,
        depth,
        chevron,
        icon,
        &node.name,
        selected,
        (dim, mark.as_ref().map(|(color, _)| *color)),
        renaming,
    );
    let response = match mark {
        Some((_, text)) => response.on_hover_text(text),
        None => response,
    };
    if done.is_some() {
        cx.rename_done = done;
    }
    if cx.scroll_to.as_ref() == Some(&node.path) {
        response.scroll_to_me(None);
    }
    if response.clicked() {
        if is_dir && !cx.open.remove(&node.path) {
            cx.open.insert(node.path.clone());
        }
        cx.action = Some(TreeAction::Select(node.path.clone()));
    }
    if cx.editable {
        if response.secondary_clicked() {
            cx.action = Some(TreeAction::Select(node.path.clone()));
        }
        response.context_menu(|ui| {
            let dir = if is_dir {
                node.path.clone()
            } else {
                node.path.parent().unwrap_or(&node.path).to_path_buf()
            };
            create_items(ui, &dir, &mut cx.action);
            ui.separator();
            if menu_item(ui, "Rename", None).clicked() {
                cx.action = Some(TreeAction::Op(FileOp::Rename(node.path.clone())));
                ui.close();
            }
            if menu_item(ui, "Delete", None).clicked() {
                cx.action = Some(TreeAction::Op(FileOp::Delete(node.path.clone())));
                ui.close();
            }
        });
    }
}

/// A tree row; while `renaming`, its name is a text field. Returns the row response and,
/// when the rename ends this frame, whether to commit it.
#[allow(clippy::too_many_arguments)]
fn tree_row(
    ui: &mut Ui,
    depth: usize,
    chevron: Option<Icon>,
    icon: Icon,
    name: &str,
    selected: bool,
    (dim, mark): (bool, Option<egui::Color32>),
    renaming: Option<&mut Rename>,
) -> (Response, Option<bool>) {
    // A build mark colors the name; see docs/features/gui.md#build-marks.
    let text = match mark {
        Some(color) => color,
        None if dim => TEXT_WEAK,
        None => TEXT,
    };
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.25));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, BG_HEADER);
    }
    let icon_rect =
        |x: f32| Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0));
    let x = rect.left() + 6.0 + depth as f32 * 14.0;
    if let Some(chevron) = chevron {
        chevron
            .image(14.0, TEXT_WEAK)
            .paint_at(ui, icon_rect(x).shrink(1.0));
    }
    icon.image(16.0, TEXT_WEAK)
        .paint_at(ui, icon_rect(x + 16.0));
    let text_rect = Rect::from_min_max(pos2(x + 36.0, rect.top() + 1.0), rect.max - vec2(4.0, 1.0));
    let Some(rename) = renaming else {
        ui.painter().text(
            pos2(x + 38.0, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(13.0),
            text,
        );
        return (response, None);
    };
    let mut output = None;
    ui.scope_builder(UiBuilder::new().max_rect(text_rect), |ui| {
        output = Some(
            egui::TextEdit::singleline(&mut rename.text)
                .font(FontId::proportional(13.0))
                .desired_width(text_rect.width())
                .margin(Margin::symmetric(2, 1))
                .show(ui),
        );
    });
    let mut output = output.expect("text field shown");
    if !rename.started {
        rename.started = true;
        output.response.request_focus();
        let all = egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(rename.text.chars().count()),
        );
        output.state.cursor.set_char_range(Some(all));
        output.state.store(ui.ctx(), output.response.id);
        return (response, None);
    }
    let done = output.response.lost_focus().then(|| {
        // Enter or clicking elsewhere commits; Escape cancels.
        !ui.input(|i| i.key_pressed(egui::Key::Escape))
    });
    (response, done)
}

/// Runs a file operation from the Input tree; failures go to the status bar.
fn apply_file_op(state: &mut UiState, project: &Project, op: FileOp) {
    let Some(input) = project.input_dir() else {
        return;
    };
    state.notice = None;
    let created = match op {
        FileOp::NewFolder(dir) => fileops::create_folder(&input, &dir)
            .map_err(|e| ("New Folder", e))
            .map(|path| (dir, path)),
        FileOp::NewNoise(dir, kind) => fileops::create_noise(&input, &dir, kind)
            .map_err(|e| ("New Noise", e))
            .map(|path| (dir, path)),
        FileOp::NewGradient(dir) => fileops::create_gradient(&input, &dir)
            .map_err(|e| ("New Gradient", e))
            .map(|path| (dir, path)),
        FileOp::NewPalette(dir) => fileops::create_palette(&input, &dir)
            .map_err(|e| ("New Palette", e))
            .map(|path| (dir, path)),
        FileOp::NewSchema(dir) => fileops::create_schema(&input, &dir)
            .map_err(|e| ("New Schema", e))
            .map(|path| (dir, path)),
        FileOp::NewGraph(dir) => fileops::create_graph(&input, &dir)
            .map_err(|e| ("New Directed Graph", e))
            .map(|path| (dir, path)),
        FileOp::NewTree(dir) => fileops::create_tree(&input, &dir)
            .map_err(|e| ("New Tree", e))
            .map(|path| (dir, path)),
        FileOp::NewMaterial(dir) => fileops::create_material(&input, &dir)
            .map_err(|e| ("New Material", e))
            .map(|path| (dir, path)),
        FileOp::NewData(dir) => fileops::create_data(&input, &dir)
            .map_err(|e| ("New Instance", e))
            .map(|path| (dir, path)),
        FileOp::Rename(path) => {
            let text = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            state.rename = Some(Rename {
                path,
                text,
                started: false,
            });
            return;
        }
        FileOp::Delete(path) => {
            state.confirm_delete = Some(path);
            return;
        }
    };
    match created {
        Ok((dir, path)) => {
            state.open.insert(dir);
            state.rescan = true;
            select(state, Location::Input, path.clone());
            apply_file_op(state, project, FileOp::Rename(path));
        }
        Err((what, e)) => state.notice = Some(format!("{what} failed: {e}")),
    }
}

fn commit_rename(state: &mut UiState, project: &Project) {
    let (Some(rename), Some(input)) = (state.rename.take(), project.input_dir()) else {
        return;
    };
    match fileops::rename(&input, &rename.path, rename.text.trim()) {
        Ok(new) if new != rename.path => {
            if state.open.remove(&rename.path) {
                state.open.insert(new.clone());
            }
            state.rescan = true;
            if state
                .selection
                .as_ref()
                .is_some_and(|s| s.path == rename.path)
            {
                select(state, Location::Input, new);
            }
        }
        Ok(_) => {}
        Err(e) => state.notice = Some(format!("Rename failed: {e}")),
    }
}

/// A modal confirmation dialog, see docs/features/gui.md#dialogs. Returns `Some(true)` when
/// `confirm` is clicked, `Some(false)` when cancelled, `None` while open.
fn dialog(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    message: &str,
    confirm: &str,
) -> Option<bool> {
    let frame = Frame::window(&ctx.global_style())
        .fill(BG_PANEL)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(8)
        .inner_margin(24);
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(title).size(16.0).color(TEXT));
            ui.add_space(8.0);
            ui.label(RichText::new(message).size(14.0).color(TEXT_WEAK));
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let button = |text: &str, color, fill, stroke| {
                    egui::Button::new(RichText::new(text).size(14.0).color(color))
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(6)
                        .min_size(vec2(96.0, 34.0))
                };
                let confirm = button(confirm, BG_BAR, ACCENT, Stroke::NONE);
                if ui.add(confirm).clicked() {
                    answer = Some(true);
                }
                let cancel = button("Cancel", TEXT, BG_HEADER, Stroke::new(1.0, STROKE));
                if ui.add(cancel).clicked() {
                    answer = Some(false);
                }
            });
        });
    if answer.is_none() && modal.should_close() {
        answer = Some(false);
    }
    answer
}

/// The delete confirmation dialog, see docs/features/file-operations.md#delete.
fn confirm_delete(ctx: &egui::Context, state: &mut UiState, project: &Project) {
    let Some(path) = state.confirm_delete.clone() else {
        return;
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let message = if path.is_dir() {
        "This permanently deletes it and everything inside it."
    } else {
        "This permanently deletes the file."
    };
    let answer = dialog(
        ctx,
        "confirm_delete",
        &format!("Delete {name}?"),
        message,
        "Delete",
    );
    if answer == Some(true) {
        state.confirm_delete = None;
        state.notice = None;
        let Some(input) = project.input_dir() else {
            return;
        };
        match fileops::delete(&input, &path) {
            Ok(()) => {
                state.rescan = true;
                if state
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.path.starts_with(&path))
                {
                    state.selection = None;
                    state.reinspect = true;
                }
            }
            Err(e) => state.notice = Some(format!("Delete failed: {e}")),
        }
    } else if answer == Some(false) {
        state.confirm_delete = None;
    }
}

fn right_sidebar(
    ui: &mut Ui,
    state: &mut UiState,
    input: Option<PathBuf>,
    assets: &Assets,
    frames: Option<&ModelFrames>,
    playhead: Option<f32>,
) {
    claim_keys(ui, &mut state.keys, KeyArea::Inspector);
    section_header(ui, "Inspector");
    // Only applicable tabs are shown; fall back to Info when the active one does not apply to
    // the inspected file (a selection still loading keeps it).
    let selection = state.selection.as_ref();
    let inspected = state.inspected.as_ref();
    if (inspected.is_some() || selection.is_none())
        && !state.inspector_tab.enabled(selection, inspected)
    {
        state.inspector_tab = InspectorTab::Info;
    }

    egui::Panel::left("inspector_tabs")
        .exact_size(36.0)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::new().fill(BG_BAR))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter()
                .vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1.0, STROKE));
            ui.spacing_mut().item_spacing.y = 0.0;
            for tab in InspectorTab::ALL {
                if !tab.enabled(state.selection.as_ref(), state.inspected.as_ref()) {
                    continue;
                }
                let response = icon_tab(ui, tab.icon(), state.inspector_tab == tab);
                if response.on_hover_text(tab.name()).clicked() {
                    state.inspector_tab = tab;
                }
            }
        });

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_PANEL))
        .show(ui, |ui| {
            match (state.inspector_tab, &mut state.inspected, &state.selection) {
                (_, _, None) => placeholder(ui, "Nothing selected"),
                (_, None, Some(_)) => placeholder(ui, "Loading…"),
                (InspectorTab::Modifiers, Some(inspected), Some(_))
                    if inspected.sidecar.is_some() =>
                {
                    if let Some(amod) = image_modifiers_tab(ui, inspected) {
                        match amod::write(&inspected.path, &amod) {
                            Ok(()) => {
                                state.reinspect = true;
                                state.rescan = true;
                            }
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Parameters, Some(inspected), Some(_))
                    if inspected.mesh.is_some() =>
                {
                    if let Some(amod) = mesh_tab(ui, inspected) {
                        match amod::write(&inspected.path, &amod) {
                            Ok(()) => {
                                state.reinspect = true;
                                state.rescan = true;
                                state.keep_model = true;
                            }
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::VertexColors, Some(inspected), Some(_))
                    if inspected.compiled_mesh.is_some() =>
                {
                    let choice = state.mesh_colors.entry(inspected.path.clone()).or_default();
                    if let Some(mesh) = &inspected.compiled_mesh {
                        mesh_colors_tab(ui, mesh, choice);
                    }
                }
                (
                    tab @ (InspectorTab::Locators
                    | InspectorTab::Shapes
                    | InspectorTab::Regions
                    | InspectorTab::Paths
                    | InspectorTab::VertexColors
                    | InspectorTab::Properties),
                    Some(inspected),
                    Some(_),
                ) => {
                    let animating = state
                        .animations
                        .get(&inspected.path)
                        .copied()
                        .flatten()
                        .is_some();
                    let tools = &mut state.locators;
                    let save = match tab {
                        InspectorTab::Shapes => shapes_tab(ui, inspected, tools, animating),
                        InspectorTab::Regions => regions_tab(ui, inspected, tools),
                        InspectorTab::Paths => paths_tab(ui, inspected, tools, animating),
                        InspectorTab::VertexColors => vertex_colors_tab(ui, inspected, tools),
                        InspectorTab::Properties => properties_tab(ui, inspected),
                        _ => locators_tab(ui, inspected, frames, tools, animating),
                    };
                    if let Some(amod) = save {
                        match amod::write(&inspected.path, &amod) {
                            Ok(()) => {
                                state.reinspect = true;
                                state.rescan = true;
                                state.keep_model = true;
                            }
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (
                    tab @ (InspectorTab::Parameters | InspectorTab::Colors),
                    Some(inspected),
                    Some(_),
                ) if inspected.palette.is_some() => {
                    let save = if tab == InspectorTab::Parameters {
                        palette_output_tab(ui, inspected)
                    } else {
                        colors_tab(ui, inspected)
                    };
                    if let Some(palette) = save {
                        match std::fs::write(&inspected.path, palette.to_toml()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (
                    tab @ (InspectorTab::Parameters | InspectorTab::Gradients),
                    Some(inspected),
                    Some(_),
                ) if inspected.gradient.is_some() => {
                    let save = if tab == InspectorTab::Parameters {
                        gradient_output_tab(ui, inspected)
                    } else {
                        gradients_tab(ui, inspected)
                    };
                    if let Some(gradients) = save {
                        match std::fs::write(&inspected.path, gradients.to_toml()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (
                    tab @ (InspectorTab::Parameters | InspectorTab::Modifiers),
                    Some(inspected),
                    Some(_),
                ) => {
                    let save = if tab == InspectorTab::Parameters {
                        parameters_tab(ui, inspected)
                    } else {
                        modifiers_tab(ui, inspected)
                    };
                    if let Some(noise) = save {
                        match std::fs::write(&inspected.path, noise.to_toml()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Gradients, Some(_), Some(_)) => placeholder(ui, "No gradients"),
                (InspectorTab::Colors, Some(_), Some(_)) => placeholder(ui, "No colors"),
                (InspectorTab::Graph | InspectorTab::Tree, Some(inspected), Some(_)) => {
                    let view = state.graph_views.entry(inspected.path.clone()).or_default();
                    if let Some(graph) = graph_tab(ui, inspected, view) {
                        match std::fs::write(&inspected.path, graph) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Atlas, Some(inspected), Some(_)) => {
                    if let Some(config) = atlas_tab(ui, inspected) {
                        let path = inspected.path.join(crate::atlas::CONFIG);
                        match std::fs::write(&path, config.to_toml()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Instances, Some(inspected), Some(_)) => {
                    let chosen = state
                        .data_instances
                        .entry(inspected.path.clone())
                        .or_default();
                    if let Some(text) = instances_tab(ui, inspected, chosen) {
                        match std::fs::write(&inspected.path, text) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Types, Some(inspected), Some(_)) => {
                    let chosen = state
                        .schema_types
                        .entry(inspected.path.clone())
                        .or_default();
                    if let Some(schema) = types_tab(ui, inspected, chosen) {
                        match std::fs::write(&inspected.path, schema.to_json()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Structs, Some(inspected), Some(_)) => {
                    let view = state
                        .structs_views
                        .entry(inspected.path.clone())
                        .or_default();
                    let save = structs_edit::structs_tab(ui, inspected, view, input.as_deref());
                    let written = match save {
                        None => None,
                        Some(structs_edit::StructsSave::File(text)) => {
                            Some(std::fs::write(&inspected.path, text).map_err(|e| e.to_string()))
                        }
                        Some(structs_edit::StructsSave::Atlas(text)) => {
                            let path = inspected.path.join(crate::atlas::CONFIG);
                            Some(std::fs::write(path, text).map_err(|e| e.to_string()))
                        }
                        Some(structs_edit::StructsSave::Sidecar(amod)) => {
                            state.rescan = true;
                            state.keep_model = true;
                            Some(amod::write(&inspected.path, &amod).map_err(|e| e.to_string()))
                        }
                    };
                    match written {
                        Some(Ok(())) => state.reinspect = true,
                        Some(Err(e)) => state.notice = Some(format!("Saving failed: {e}")),
                        None => {}
                    }
                }
                (InspectorTab::Material, Some(inspected), Some(_)) => {
                    if let Some(source) = material_tab(ui, inspected) {
                        match std::fs::write(&inspected.path, source.to_toml()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Lods, Some(inspected), Some(_)) => {
                    let choice = state.lod_levels.entry(inspected.path.clone()).or_default();
                    if let Some(mesh) = &inspected.compiled_mesh {
                        lods_tab(ui, mesh, choice);
                    }
                }
                (InspectorTab::Mipmaps, Some(inspected), Some(_)) => {
                    let choice = state.mip_levels.entry(inspected.path.clone()).or_default();
                    mipmaps_tab(ui, &inspected.levels, choice);
                }
                (InspectorTab::Animations, Some(inspected), Some(_)) => {
                    let choice = state.animations.entry(inspected.path.clone()).or_default();
                    let playhead = playhead.unwrap_or(0.0);
                    if let Some(amod) = animations_tab(ui, inspected, choice, playhead) {
                        match amod::write(&inspected.path, &amod) {
                            Ok(()) => {
                                state.reinspect = true;
                                state.rescan = true;
                                state.keep_model = true;
                            }
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Info, Some(inspected), _) => info(ui, &inspected.inspection),
                (InspectorTab::Assets, Some(inspected), Some(selection)) => {
                    let texture = inspected.inspection.texture_type.as_deref();
                    assets_tab(ui, assets, &selection.path, texture)
                }
            }
        });
}

/// An `.anoise` file being edited in the Parameters / Modifiers tabs.
struct NoiseEdit {
    edit: Noise,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    saved: Noise,
}

/// The noise being edited, or a placeholder with why there is none.
fn noise_edit<'a>(ui: &mut Ui, inspected: &'a mut Inspected) -> Option<&'a mut NoiseEdit> {
    match &mut inspected.noise {
        Some(Ok(edit)) => Some(edit),
        Some(Err(e)) => {
            placeholder(ui, e);
            None
        }
        None => {
            placeholder(ui, "No parameters");
            None
        }
    }
}

/// Returns the noise to write when an edit is committed: once nothing is being dragged or
/// typed into, so sliders and fields save on release. See docs/features/noise.md#saving.
fn noise_commit(ui: &Ui, edit: &mut NoiseEdit) -> Option<Noise> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

/// Field look for the property editor: `BG_BAR` fill, 4 px corners.
fn property_style(ui: &mut Ui) {
    let visuals = ui.visuals_mut();
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = 4.into();
    }
    visuals.widgets.inactive.bg_fill = BG_BAR;
    visuals.widgets.inactive.weak_bg_fill = BG_BAR;
    visuals.extreme_bg_color = BG_BAR;
    ui.spacing_mut().item_spacing.y = 6.0;
}

/// Whether a section card folds, and how it starts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fold {
    Never,
    Open,
    Folded,
}

/// A section card, see docs/features/gui.md#property-editor. When `collapsible`, a chevron
/// starts the header; `leading` draws next (before the title), `trailing` on the right, told
/// whether the card is open.
#[allow(clippy::too_many_arguments)]
fn prop_section(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    fold: Fold,
    title: RichText,
    leading: impl FnOnce(&mut Ui),
    trailing: impl FnOnce(&mut Ui, bool),
    body: impl FnOnce(&mut Ui),
) -> Rect {
    let id = ui.make_persistent_id(id);
    let open = (fold != Fold::Never)
        .then(|| ui.data(|d| d.get_temp::<bool>(id).unwrap_or(fold == Fold::Open)));
    let (rect, toggled) = card(ui, open, title, leading, trailing, body);
    if toggled && let Some(open) = open {
        ui.data_mut(|d| d.insert_temp(id, !open));
    }
    rect
}

/// A section card whose fold state the caller keeps: `open` is `None` for one that does not
/// fold. Returns its rectangle and whether the chevron or title was clicked.
fn card(
    ui: &mut Ui,
    open: Option<bool>,
    title: RichText,
    leading: impl FnOnce(&mut Ui),
    trailing: impl FnOnce(&mut Ui, bool),
    body: impl FnOnce(&mut Ui),
) -> (Rect, bool) {
    let collapsible = open.is_some();
    let open = open.unwrap_or(true);
    let mut toggled = false;
    let card = Frame::new()
        .fill(BG_HEADER)
        .corner_radius(6)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                vec2(width, 28.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(6.0);
                    let chevron = collapsible.then(|| {
                        let (rect, chevron) =
                            ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                        let icon = if open {
                            Icon::ChevronDown
                        } else {
                            Icon::ChevronRight
                        };
                        icon.image(14.0, TEXT_WEAK).paint_at(ui, rect.shrink(1.0));
                        chevron
                    });
                    leading(ui);
                    let title = egui::Label::new(title.size(14.0)).selectable(false);
                    let title = ui.add(if collapsible {
                        title.sense(Sense::click())
                    } else {
                        title
                    });
                    toggled =
                        chevron.is_some_and(|c| c.clicked()) || (collapsible && title.clicked());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_space(4.0);
                        trailing(ui, open);
                    });
                },
            );
            if open {
                Frame::new()
                    .inner_margin(Margin {
                        left: 10,
                        right: 10,
                        top: 2,
                        bottom: 10,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        body(ui);
                    });
            }
        });
    ui.add_space(8.0);
    (card.response.rect, toggled)
}

/// A row: `label` right-aligned in the label column, then `control` given the rest of the width.
fn prop_row(ui: &mut Ui, label: &str, control: impl FnOnce(&mut Ui, f32)) {
    prop_row_colored(ui, label, TEXT_WEAK, None, control);
}

/// [`prop_row`] with the label in `color`, explained by `hint` on hover.
fn prop_row_colored(
    ui: &mut Ui,
    label: &str,
    color: egui::Color32,
    hint: Option<&str>,
    control: impl FnOnce(&mut Ui, f32),
) {
    let width = ui.available_width();
    let label_width = (width * 0.4).max(80.0);
    ui.allocate_ui_with_layout(
        vec2(width, 26.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rect, response) =
                ui.allocate_exact_size(vec2(label_width - 8.0, 26.0), Sense::hover());
            if let Some(hint) = hint {
                response.on_hover_text(hint);
            }
            ui.painter().text(
                rect.right_center(),
                Align2::RIGHT_CENTER,
                label,
                FontId::proportional(13.0),
                color,
            );
            control(ui, (width - label_width).max(40.0));
        },
    );
}

/// Slider plus a 56 px value box filling `width`.
fn slider_box<N: egui::emath::Numeric>(
    ui: &mut Ui,
    width: f32,
    value: &mut N,
    range: std::ops::RangeInclusive<N>,
    decimals: Option<usize>,
) {
    ui.spacing_mut().slider_width = (width - 64.0).max(20.0);
    let mut slider = egui::Slider::new(value, range.clone()).show_value(false);
    if let Some(d) = decimals {
        slider = slider.fixed_decimals(d);
    }
    ui.add(slider);
    let mut field = egui::DragValue::new(value).range(range);
    if let Some(d) = decimals {
        field = field.fixed_decimals(d).speed(0.01);
    }
    ui.add_sized(vec2(56.0, 22.0), field);
}

/// One parameter row, driven by its declaration.
fn param_row(ui: &mut Ui, spec: &ParamSpec, value: &mut Value) {
    prop_row(ui, spec.label, |ui, width| match (spec.kind, value) {
        (ParamKind::Int { min, max, .. }, Value::Int(v)) if max - min > 256 => {
            ui.add_sized(vec2(width, 22.0), egui::DragValue::new(v).range(min..=max));
        }
        (ParamKind::Int { min, max, .. }, Value::Int(v)) => {
            slider_box(ui, width, v, min..=max, None)
        }
        (ParamKind::Float { min, max, .. }, Value::Float(v)) => {
            slider_box(ui, width, v, min..=max, Some(2))
        }
        (ParamKind::Gray { .. }, Value::Float(v)) => slider_box(ui, width, v, 0.0..=1.0, Some(2)),
        (ParamKind::Choice { options, .. }, Value::Choice(i)) => {
            egui::ComboBox::from_id_salt(spec.key)
                .width(width - 8.0)
                .selected_text(options[*i])
                .show_ui(ui, |ui| {
                    for (k, option) in options.iter().enumerate() {
                        ui.selectable_value(i, k, *option);
                    }
                });
        }
        _ => {
            ui.weak("—");
        }
    });
}

/// Generator and its parameters; see docs/features/noise.md#parameters-tab.
fn parameters_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Noise> {
    let edit = noise_edit(ui, inspected)?;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let noise = &mut edit.edit;
                let mut kind = noise.kind;
                prop_section(
                    ui,
                    "generator",
                    Fold::Open,
                    RichText::new("Generator"),
                    |_| {},
                    |_, _| {},
                    |ui| {
                        prop_row(ui, "Type", |ui, width| {
                            egui::ComboBox::from_id_salt("noise_type")
                                .width(width - 8.0)
                                .selected_text(kind.name())
                                .show_ui(ui, |ui| {
                                    for k in GeneratorKind::ALL {
                                        ui.selectable_value(&mut kind, k, k.name());
                                    }
                                });
                        });
                        prop_row(ui, "Size", |ui, width| {
                            egui::ComboBox::from_id_salt("noise_size")
                                .width(width - 8.0)
                                .selected_text(noise.size.to_string())
                                .show_ui(ui, |ui| {
                                    for size in [4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
                                    {
                                        ui.selectable_value(
                                            &mut noise.size,
                                            size,
                                            size.to_string(),
                                        );
                                    }
                                });
                        });
                    },
                );
                // The file keeps each type's parameters in its own section.
                noise.set_kind(kind);
                let specs = noise.kind.specs();
                prop_section(
                    ui,
                    ("generator_params", noise.kind),
                    Fold::Open,
                    RichText::new(noise.kind.name()),
                    |_| {},
                    |_, _| {},
                    |ui| {
                        for (spec, value) in specs.iter().zip(&mut noise.params.0) {
                            param_row(ui, spec, value);
                        }
                    },
                );
            });
        });
    noise_commit(ui, edit)
}

/// An image's `.amod` being edited; see docs/features/textures.md#modifiers.
struct SidecarEdit {
    edit: Amod,
    /// As last written.
    saved: Amod,
}

/// A model's `.amod` mesh settings being edited; see docs/features/models.md#parameters-tab.
struct MeshEdit {
    edit: Amod,
    /// As last written.
    saved: Amod,
    /// What `assetc.toml` gives the model, shown where the sidecar sets nothing.
    project: MeshOptions,
    /// The chosen locator, shape and region, by index; see
    /// docs/features/locators.md#locators-tab.
    locator: Option<usize>,
    shape: Option<usize>,
    region: Option<usize>,
    /// The chosen path and its chosen point.
    path: Option<usize>,
    point: Option<usize>,
    /// The chosen animation event, by index in the sidecar's.
    event: Option<usize>,
    /// The chosen color set, its chosen gradient end (0 From, 1 To) and each authored set's
    /// gradient editor; see docs/features/vertex-colors.md#vertex-colors-tab.
    color: Option<usize>,
    color_end: usize,
    color_cards: Vec<GradientCard>,
    /// The locators the model has without its sidecar; `edit` and `saved` hold every locator,
    /// and only what differs from these is written.
    inherited: Vec<Inherited>,
    /// The locators of the models shown at its locators.
    held: HeldLocators,
}

/// The locators of models shown at locators, by input-relative path, each read when first
/// needed; see docs/features/locators.md#attachment-preview.
#[derive(Default)]
struct HeldLocators {
    input: Option<PathBuf>,
    presets: crate::config::Locators,
    models: HashMap<String, Result<Vec<amod::Locator>, String>>,
}

impl HeldLocators {
    fn get(&mut self, rel: &str) -> &Result<Vec<amod::Locator>, String> {
        let Self {
            input,
            presets,
            models,
        } = self;
        models.entry(rel.to_owned()).or_insert_with(|| match input {
            Some(input) => presets::model_locators(presets, input, rel),
            None => Err("the project has no input folder".into()),
        })
    }

    /// `rel`'s locator `name`: its joint and transform there.
    fn find(
        &mut self,
        rel: &str,
        name: &str,
    ) -> Option<(Option<String>, bevy::prelude::Transform)> {
        let found = self
            .get(rel)
            .as_ref()
            .ok()?
            .iter()
            .find(|l| l.name == name)?;
        let local = bevy::math::Mat4::from(gizmo::local(found));
        Some((
            found.joint.clone(),
            bevy::prelude::Transform::from_matrix(local),
        ))
    }
}

impl MeshEdit {
    /// The model's whole list of color sets, whose glTF sets are `model`; see
    /// docs/features/vertex-colors.md#order.
    fn color_sets(&self, model: &[u32]) -> Vec<enrich::ColorSet> {
        vertex_colors::full_list(&self.edit.colors, model)
    }

    /// Keeps what the sidecar needs of the whole list `full`.
    fn set_color_sets(&mut self, full: &[enrich::ColorSet], model: &[u32]) {
        let part = vertex_colors::sidecar_part(full, model);
        if part != self.edit.colors {
            self.edit.colors = part;
        }
    }

    /// Where locator `i` comes from, when it is inherited; and whether the sidecar changes it.
    fn inherited(&self, i: usize) -> Option<(&Inherited, bool)> {
        let locator = self.edit.locators.get(i)?;
        let from = self
            .inherited
            .iter()
            .find(|f| f.locator.name == locator.name)?;
        Some((from, from.locator != *locator))
    }
}

/// Label color of a mesh setting: `ACCENT` when the sidecar sets it.
fn set_color(set: bool) -> egui::Color32 {
    if set { ACCENT } else { TEXT_WEAK }
}

/// An integer setting: the sidecar's value, or the project's until edited.
fn patch_int(
    ui: &mut Ui,
    label: &str,
    hint: &str,
    value: &mut Option<u32>,
    project: u32,
    range: std::ops::RangeInclusive<u32>,
    speed: f64,
) {
    prop_row_colored(
        ui,
        label,
        set_color(value.is_some()),
        Some(hint),
        |ui, width| {
            let mut v = value.unwrap_or(project);
            let before = v;
            let field = egui::DragValue::new(&mut v).range(range).speed(speed);
            ui.add_sized(vec2(width, 22.0), field);
            // Only a new value is an override; a click alone writes nothing.
            if v != before {
                *value = Some(v);
            }
        },
    );
}

/// A float setting as a slider with a value box.
fn patch_float(
    ui: &mut Ui,
    label: &str,
    hint: &str,
    value: &mut Option<f64>,
    project: f64,
    range: std::ops::RangeInclusive<f64>,
    decimals: usize,
) {
    prop_row_colored(
        ui,
        label,
        set_color(value.is_some()),
        Some(hint),
        |ui, width| {
            let mut v = value.unwrap_or(project);
            let before = v;
            slider_box(ui, width, &mut v, range, Some(decimals));
            if v != before {
                let scale = 10f64.powi(decimals as i32);
                *value = Some((v * scale).round() / scale);
            }
        },
    );
}

/// A boolean setting as a checkbox.
fn patch_bool(ui: &mut Ui, label: &str, hint: &str, value: &mut Option<bool>, project: bool) {
    prop_row_colored(
        ui,
        label,
        set_color(value.is_some()),
        Some(hint),
        |ui, _| {
            let mut v = value.unwrap_or(project);
            if ui.checkbox(&mut v, "").changed() {
                *value = Some(v);
            }
        },
    );
}

/// Meshoptimizer settings of a model, written to its sidecar; see
/// docs/features/models.md#parameters-tab.
fn mesh_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Amod> {
    let name = inspected.inspection.name.clone();
    let edit = match &mut inspected.mesh {
        Some(Ok(edit)) => edit,
        Some(Err(e)) => {
            placeholder(ui, e);
            return None;
        }
        None => {
            placeholder(ui, "No parameters");
            return None;
        }
    };
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let project = edit.project.clone();
                let mesh = &mut edit.edit.mesh;
                prop_section(
                    ui,
                    "mesh_meshlets",
                    Fold::Open,
                    RichText::new("Meshlets"),
                    |_| {},
                    |_, _| {},
                    |ui| {
                        patch_int(
                            ui,
                            "Max vertices",
                            "Most vertices one meshlet may use. Match what your mesh shader outputs per workgroup (64 is common).",
                            &mut mesh.meshlet_max_vertices,
                            project.meshlet_max_vertices,
                            3..=256,
                            1.0,
                        );
                        patch_int(
                            ui,
                            "Max triangles",
                            "Most triangles one meshlet may hold, a multiple of 4. Match what your mesh shader outputs per workgroup (124 is common).",
                            &mut mesh.meshlet_max_triangles,
                            project.meshlet_max_triangles,
                            4..=512,
                            4.0,
                        );
                        // Meshoptimizer needs a multiple of 4; rounded once the edit is done.
                        if !busy && let Some(t) = &mut mesh.meshlet_max_triangles {
                            *t = ((*t + 2) / 4 * 4).clamp(4, 512);
                        }
                        patch_float(
                            ui,
                            "Cone weight",
                            "0 packs triangles into as few meshlets as possible. Higher groups triangles that face the same way, so whole meshlets facing away from the camera can be skipped.",
                            &mut mesh.meshlet_cone_weight,
                            project.meshlet_cone_weight,
                            0.0..=1.0,
                            2,
                        );
                    },
                );
                prop_section(
                    ui,
                    "mesh_lods",
                    Fold::Open,
                    RichText::new("Levels of detail"),
                    |_| {},
                    |_, _| {},
                    |ui| {
                        let set = mesh.lod_ratios.is_some();
                        let mut ratios = mesh
                            .lod_ratios
                            .clone()
                            .unwrap_or_else(|| project.lod_ratios.clone());
                        let before = ratios.clone();
                        let mut count = ratios.len() as u32;
                        prop_row_colored(ui, "Levels", set_color(set), Some("How many simplified copies of the mesh to build, for drawing it far away. 0 builds none."), |ui, width| {
                            let field = egui::DragValue::new(&mut count).range(0..=8);
                            ui.add_sized(vec2(width, 22.0), field);
                        });
                        while ratios.len() > count as usize {
                            ratios.pop();
                        }
                        while ratios.len() < count as usize {
                            let half = ratios.last().copied().unwrap_or(1.0) / 2.0;
                            ratios.push((half * 100.0).round().max(1.0) / 100.0);
                        }
                        // Each level stays below the one before.
                        let mut ceiling = 1.0;
                        for (i, ratio) in ratios.iter_mut().enumerate() {
                            prop_row_colored(
                                ui,
                                &format!("LOD {}", i + 1),
                                set_color(set),
                                Some("Size of this level as a share of the full mesh's triangles (0.25 = a quarter). Each level must be smaller than the one before."),
                                |ui, width| {
                                    slider_box(ui, width, ratio, 0.01..=0.99, Some(2));
                                },
                            );
                            *ratio = ((*ratio * 100.0).round() / 100.0)
                                .min(((ceiling - 0.01) * 100.0f64).round() / 100.0)
                                .max(0.01);
                            ceiling = *ratio;
                        }
                        if ratios != before {
                            mesh.lod_ratios = Some(ratios);
                        }
                        patch_float(
                            ui,
                            "Error",
                            "How far simplification may move the surface, as a share of the mesh's size (0.05 = 5%). Lower keeps the shape better but may stop before reaching the target size.",
                            &mut mesh.lod_error,
                            project.lod_error,
                            0.0..=1.0,
                            3,
                        );
                        patch_float(
                            ui,
                            "Error step",
                            "How much the allowed error grows on each retry, when a level cannot get smaller.",
                            &mut mesh.lod_error_step,
                            project.lod_error_step,
                            0.0..=1.0,
                            3,
                        );
                        patch_int(
                            ui,
                            "Attempts",
                            "How many times to retry with more allowed error before giving up (or falling back to sloppy).",
                            &mut mesh.lod_attempts,
                            project.lod_attempts,
                            0..=10,
                            0.1,
                        );
                        patch_float(
                            ui,
                            "UV weight",
                            "How much keeping texture coordinates matters. Higher keeps textures from stretching; lower simplifies more freely.",
                            &mut mesh.lod_uv_weight,
                            project.lod_uv_weight,
                            0.0..=10.0,
                            2,
                        );
                        patch_float(
                            ui,
                            "Normal weight",
                            "How much keeping normals (the shading) matters. Higher keeps the lighting; lower simplifies more freely.",
                            &mut mesh.lod_normal_weight,
                            project.lod_normal_weight,
                            0.0..=10.0,
                            2,
                        );
                        patch_bool(
                            ui,
                            "Sloppy fallback",
                            "When careful simplification cannot reach the target, merge nearby vertices regardless of how the mesh is built. Always gets smaller, but can stretch textures and leave holes. Off: the level reuses the previous one.",
                            &mut mesh.lod_sloppy_fallback,
                            project.lod_sloppy_fallback,
                        );
                        patch_bool(
                            ui,
                            "Permissive",
                            "Allow simplifying across seams where UVs or normals split, such as every edge of a flat-shaded model. Flat-shaded models need it; the UV and normal weights keep it in check.",
                            &mut mesh.lod_permissive,
                            project.lod_permissive,
                        );
                        patch_bool(
                            ui,
                            "Lock border",
                            "Keep vertices on the mesh's open edges in place, so pieces that meet along their edges (such as terrain tiles) stay sealed.",
                            &mut mesh.lod_lock_border,
                            project.lod_lock_border,
                        );
                        patch_bool(
                            ui,
                            "Regularize",
                            "Prefer evenly sized triangles, at some cost to accuracy.",
                            &mut mesh.lod_regularize,
                            project.lod_regularize,
                        );
                        patch_bool(ui, "Prune", "Remove small, disconnected pieces while simplifying, when they are too small to matter.", &mut mesh.lod_prune, project.lod_prune);
                    },
                );
                ui.weak(if mesh.is_empty() {
                    "Project settings (assetc.toml)".to_owned()
                } else {
                    format!("Set for this file ({name}.amod)")
                });
                ui.add_space(4.0);
                let reset =
                    ui.add_enabled(!mesh.is_empty(), egui::Button::new("Use project settings"));
                if reset.clicked() {
                    *mesh = Default::default();
                }
            });
        });
    mesh_commit(ui, edit)
}

/// The sidecar to write once it changed and nothing is being dragged or typed into, unless a
/// locator's name is empty or taken (while it is being typed).
fn mesh_commit(ui: &Ui, edit: &mut MeshEdit) -> Option<Amod> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    // Names in the making (empty or taken) wait.
    fn unique<'a>(names: impl Iterator<Item = &'a String> + Clone) -> bool {
        names
            .clone()
            .enumerate()
            .all(|(i, n)| !n.is_empty() && names.clone().take(i).all(|other| other != n))
    }
    let named = unique(edit.edit.locators.iter().map(|l| &l.name))
        && unique(edit.edit.shapes.iter().map(|s| &s.name))
        && unique(edit.edit.regions.iter().map(|r| &r.name))
        && unique(edit.edit.paths.iter().map(|p| &p.name))
        && unique(edit.edit.colors.iter().map(|c| &c.name));
    (!busy && named && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        Amod {
            locators: presets::sidecar_part(&edit.edit.locators, &edit.inherited),
            ..edit.edit.clone()
        }
    })
}

/// An `.agradient` file being edited; see docs/features/gradients.md#parameters-tab.
struct GradientEdit {
    edit: Gradients,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    saved: Gradients,
    /// Editor state of each gradient card, in the same order.
    cards: Vec<GradientCard>,
}

/// One gradient card's editor state, see docs/features/gradients.md#gradient-editor.
#[derive(Default)]
struct GradientCard {
    selected: usize,
    drag: Option<StopDrag>,
    /// When a press last added a stop, so that gesture's double-click does not remove it.
    added_at: f64,
    /// The rendered bar and folded preview: what they show, their size in pixels, the texture.
    bar: Option<(Gradient, [usize; 2], egui::TextureHandle)>,
    mini: Option<(Gradient, [usize; 2], egui::TextureHandle)>,
    picker: Picker,
}

struct StopDrag {
    index: usize,
    /// Pointer x at the press, and the stop's offset from it.
    start: f32,
    offset: f32,
    moved: bool,
    /// Pulled far enough below the handles that releasing removes it.
    removing: bool,
}

enum CardAction {
    Remove(usize),
}

/// The gradient file being edited, or a placeholder with why there is none.
fn gradient_edit<'a>(ui: &mut Ui, inspected: &'a mut Inspected) -> Option<&'a mut GradientEdit> {
    match &mut inspected.gradient {
        Some(Ok(edit)) => Some(edit),
        Some(Err(e)) => {
            placeholder(ui, e);
            None
        }
        None => {
            placeholder(ui, "No parameters");
            None
        }
    }
}

/// The gradients to write once nothing is being dragged or typed into, as for noise.
fn gradient_commit(ui: &Ui, edit: &mut GradientEdit) -> Option<Gradients> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

/// Image size, cells and layout; see docs/features/gradients.md#parameters-tab.
fn gradient_output_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Gradients> {
    let edit = gradient_edit(ui, inspected)?;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                grid_section(ui, &mut edit.edit);
            });
        });
    gradient_commit(ui, edit)
}

/// The Output section of an `.agradient` or `.apalette` file: image size, cells and layout; see
/// docs/features/gradients.md#parameters-tab.
fn grid_section(ui: &mut Ui, file: &mut impl CellGrid) {
    prop_section(
        ui,
        "grid_output",
        Fold::Open,
        RichText::new("Output"),
        |_| {},
        |_, _| {},
        |ui| {
            // A change that leaves too few cells is undone, so the file stays valid.
            let before = file.grid();
            let [width, height, ..] = before;
            let ([w, h, cw, ch], layout) = file.grid_mut();
            let fields = [
                ("Width", w, 4096),
                ("Height", h, 4096),
                ("Cell width", cw, width),
                ("Cell height", ch, height),
            ];
            for (label, value, max) in fields {
                prop_row(ui, label, |ui, width| {
                    ui.add_sized(
                        vec2(width, 22.0),
                        egui::DragValue::new(value).range(1..=max),
                    );
                });
            }
            prop_row(ui, "Layout", |ui, width| {
                egui::ComboBox::from_id_salt("grid_layout")
                    .width(width - 8.0)
                    .selected_text(layout.name())
                    .show_ui(ui, |ui| {
                        for option in GradientLayout::ALL {
                            ui.selectable_value(layout, option, option.name())
                                .on_hover_text(option.description());
                        }
                    });
            });
            let ([w, h, cw, ch], _) = file.grid_mut();
            *cw = (*cw).min(*w);
            *ch = (*ch).min(*h);
            if file.validate().is_err() {
                let ([w, h, cw, ch], _) = file.grid_mut();
                [*w, *h, *cw, *ch] = before;
            }
            ui.weak(format!(
                "{} of {} cells used ({} × {})",
                file.used(),
                file.capacity(),
                file.columns(),
                file.rows()
            ));
        },
    );
}

/// An `.apalette` file being edited; see docs/features/palettes.md#colors-tab.
struct PaletteEdit {
    edit: Palette,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    saved: Palette,
    selected: usize,
    /// The swatch being dragged to a new place.
    drag: Option<usize>,
    picker: Picker,
}

/// The palette file being edited, or a placeholder with why there is none.
fn palette_edit<'a>(ui: &mut Ui, inspected: &'a mut Inspected) -> Option<&'a mut PaletteEdit> {
    match &mut inspected.palette {
        Some(Ok(edit)) => Some(edit),
        Some(Err(e)) => {
            placeholder(ui, e);
            None
        }
        None => {
            placeholder(ui, "No parameters");
            None
        }
    }
}

/// The palette to write once nothing is being dragged or typed into, as for gradients.
fn palette_commit(ui: &Ui, edit: &mut PaletteEdit) -> Option<Palette> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

/// Image size, cells and layout; see docs/features/palettes.md#parameters-tab.
fn palette_output_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Palette> {
    let edit = palette_edit(ui, inspected)?;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                grid_section(ui, &mut edit.edit);
            });
        });
    palette_commit(ui, edit)
}

/// Swatches and the selected color's editor; see docs/features/palettes.md#colors-tab.
fn colors_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Palette> {
    const SWATCH: f32 = 24.0;
    const GAP: f32 = 4.0;
    let edit = palette_edit(ui, inspected)?;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let file = &mut edit.edit;
                let room = file.colors.len() < file.capacity();
                edit.selected = edit.selected.min(file.colors.len().saturating_sub(1));

                // Swatches, wrapping, then the add button.
                let per_row = ((ui.available_width() + GAP) / (SWATCH + GAP))
                    .floor()
                    .max(1.0) as usize;
                let count = file.colors.len() + 1;
                let rows = count.div_ceil(per_row);
                let (area, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), rows as f32 * (SWATCH + GAP) - GAP),
                    Sense::hover(),
                );
                let slot = |i: usize| {
                    Rect::from_min_size(
                        area.min
                            + vec2(
                                (i % per_row) as f32 * (SWATCH + GAP),
                                (i / per_row) as f32 * (SWATCH + GAP),
                            ),
                        vec2(SWATCH, SWATCH),
                    )
                };
                let pointer = ui.ctx().pointer_interact_pos();
                let target = edit.drag.and_then(|_| {
                    let p = pointer?;
                    (0..file.colors.len()).find(|&i| slot(i).expand(GAP / 2.0).contains(p))
                });
                let mut moved = None;
                for i in 0..file.colors.len() {
                    let rect = slot(i);
                    let response = ui
                        .interact(rect, ui.id().with(("swatch", i)), Sense::click_and_drag())
                        .on_hover_text({
                            let swatch = &file.colors[i];
                            let name = if swatch.name.is_empty() {
                                format!("Color {}", i + 1)
                            } else {
                                swatch.name.clone()
                            };
                            format!("{name} {}", gradient::format_color(swatch.color))
                        });
                    if response.clicked() || response.drag_started() {
                        edit.selected = i;
                    }
                    if response.drag_started() {
                        edit.drag = Some(i);
                    }
                    if response.drag_stopped()
                        && let Some(from) = edit.drag.take()
                        && let Some(to) = target
                    {
                        moved = Some((from, to));
                    }
                    let color = file.colors[i].color;
                    if color[3] < 255 {
                        color_picker::checker(ui, rect, 3);
                    }
                    let painter = ui.painter();
                    painter.rect_filled(rect, 3, color_picker::to_color32(color));
                    let (width, stroke) = if i == edit.selected || target == Some(i) {
                        (2.0, ACCENT)
                    } else {
                        (1.0, TEXT_WEAK)
                    };
                    painter.rect_stroke(
                        rect,
                        3,
                        Stroke::new(width, stroke),
                        egui::StrokeKind::Inside,
                    );
                }
                if edit.drag.is_some() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    if !ui.input(|i| i.pointer.any_down()) {
                        edit.drag = None;
                    }
                }
                if let Some((from, to)) = moved
                    && from != to
                {
                    let swatch = file.colors.remove(from);
                    file.colors.insert(to, swatch);
                    edit.selected = to;
                }
                // Add: a copy of the selected color's value.
                let add = slot(file.colors.len());
                let response = ui
                    .interact(add, ui.id().with("swatch_add"), sense_if(room))
                    .on_hover_text(if room {
                        "Add color"
                    } else {
                        "No room: enlarge the image or shrink the cells"
                    });
                let tint = if !room {
                    TEXT_WEAK.gamma_multiply(0.5)
                } else if response.hovered() {
                    TEXT
                } else {
                    TEXT_WEAK
                };
                ui.painter()
                    .rect_stroke(add, 3, Stroke::new(1.0, tint), egui::StrokeKind::Inside);
                Icon::Plus
                    .image(14.0, tint)
                    .paint_at(ui, Rect::from_center_size(add.center(), vec2(14.0, 14.0)));
                if room && response.clicked() {
                    let color = file.colors.get(edit.selected).map_or([255; 4], |s| s.color);
                    file.colors.push(Swatch {
                        name: format!("Color {}", file.colors.len() + 1),
                        color,
                        ..Swatch::default()
                    });
                    edit.selected = file.colors.len() - 1;
                }
                ui.add_space(12.0);

                // The selected color.
                let Some(swatch) = file.colors.get_mut(edit.selected) else {
                    ui.weak("No colors");
                    return;
                };
                prop_row(ui, "Name", |ui, width| {
                    ui.add_sized(
                        vec2(width, 22.0),
                        egui::TextEdit::singleline(&mut swatch.name),
                    );
                });
                ui.add_space(6.0);
                edit.picker.show(ui, &mut swatch.color);
                ui.add_space(6.0);
                let (mut duplicate, mut remove) = (false, false);
                ui.horizontal(|ui| {
                    duplicate = ui
                        .add_enabled(room, egui::Button::new("Duplicate"))
                        .clicked();
                    remove = ui
                        .add(egui::Button::image_and_text(
                            Icon::Trash2.image(14.0, TEXT),
                            "Remove color",
                        ))
                        .clicked();
                });
                if duplicate {
                    let copy = swatch.clone();
                    file.colors.insert(edit.selected + 1, copy);
                    edit.selected += 1;
                } else if remove {
                    file.colors.remove(edit.selected);
                    edit.selected = edit.selected.min(file.colors.len().saturating_sub(1));
                }
            });
        });
    palette_commit(ui, edit)
}

/// Clicks only when enabled; a disabled control still takes hover (for its tooltip).
fn sense_if(enabled: bool) -> Sense {
    if enabled {
        Sense::click()
    } else {
        Sense::hover()
    }
}

/// One foldable card per gradient; see docs/features/gradients.md#gradients-tab.
fn gradients_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Gradients> {
    let edit = gradient_edit(ui, inspected)?;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let file = &mut edit.edit;
                let cards = &mut edit.cards;
                cards.resize_with(file.gradients.len(), GradientCard::default);
                let action = std::cell::Cell::new(None);
                let dragging = std::cell::Cell::new(None);
                let dropped = std::cell::Cell::new(None);
                let mut rects = Vec::with_capacity(file.gradients.len());
                for (i, (gradient, card)) in
                    file.gradients.iter_mut().zip(cards.iter_mut()).enumerate()
                {
                    let title = if gradient.name.is_empty() {
                        format!("Gradient {}", i + 1)
                    } else {
                        gradient.name.clone()
                    };
                    // Folded cards show a small preview where the remove button would be.
                    let shown = gradient.clone();
                    let mut mini = card.mini.take();
                    rects.push(prop_section(
                        ui,
                        ("gradient", i),
                        Fold::Folded,
                        RichText::new(title),
                        |ui| {
                            let grip = drag_handle(ui);
                            if grip.dragged() {
                                dragging.set(Some(i));
                            }
                            if grip.drag_stopped() {
                                dropped.set(Some(i));
                            }
                        },
                        |ui, open| {
                            if open {
                                if icon_button(ui, Icon::Trash2, "Remove", true).clicked() {
                                    action.set(Some(CardAction::Remove(i)));
                                }
                            } else {
                                ui.add_space(4.0);
                                let (rect, _) =
                                    ui.allocate_exact_size(vec2(48.0, 12.0), Sense::hover());
                                color_picker::checker(ui, rect, 3);
                                let texture = gradient_texture(ui, &mut mini, &shown, rect.size());
                                egui::Image::from_texture((texture.id(), rect.size()))
                                    .corner_radius(3)
                                    .paint_at(ui, rect);
                            }
                        },
                        |ui| {
                            prop_row(ui, "Name", |ui, width| {
                                ui.add_sized(
                                    vec2(width, 22.0),
                                    egui::TextEdit::singleline(&mut gradient.name),
                                );
                            });
                            prop_row(ui, "Dither", |ui, width| {
                                egui::ComboBox::from_id_salt(("gradient_dither", i))
                                    .width(width - 8.0)
                                    .selected_text(gradient.dither.name())
                                    .show_ui(ui, |ui| {
                                        for d in Dither::ALL {
                                            ui.selectable_value(&mut gradient.dither, d, d.name());
                                        }
                                    });
                            });
                            ui.add_space(4.0);
                            ui.push_id(("gradient_editor", i), |ui| {
                                gradient_editor(ui, gradient, card);
                            });
                            // Its own modifiers; see docs/features/gradients.md#gradients-tab.
                            ui.add_space(8.0);
                            ui.label(RichText::new("Modifiers").size(13.0).color(TEXT_WEAK));
                            ui.add_space(4.0);
                            ui.push_id(("gradient_modifiers", i), |ui| {
                                modifier_cards(ui, &mut gradient.modifiers);
                            });
                        },
                    ));
                    card.mini = mini;
                }
                if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
                    let gradient = file.gradients.remove(from);
                    file.gradients.insert(to, gradient);
                    let card = cards.remove(from);
                    cards.insert(to, card);
                }
                if let Some(CardAction::Remove(i)) = action.take() {
                    file.gradients.remove(i);
                    cards.remove(i);
                }
                if file.gradients.is_empty() {
                    ui.weak("No gradients");
                    ui.add_space(6.0);
                }
                let room = file.gradients.len() < file.capacity();
                let button =
                    egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add gradient");
                let added = ui
                    .add_enabled(room, button)
                    .on_disabled_hover_text("No room: enlarge the image or shrink the cells")
                    .clicked();
                if added {
                    let name = format!("Gradient {}", file.gradients.len() + 1);
                    file.gradients.push(Gradient::new(name));
                    cards.push(GradientCard::default());
                }
            });
        });
    gradient_commit(ui, edit)
}

/// `gradient` on its side at `size` points, rendered at screen pixels; re-rendered only
/// when it or the size changed.
fn gradient_texture(
    ui: &Ui,
    cache: &mut Option<(Gradient, [usize; 2], egui::TextureHandle)>,
    gradient: &Gradient,
    size: Vec2,
) -> egui::TextureHandle {
    let ppp = ui.ctx().pixels_per_point();
    let pixels = [
        (size.x * ppp).round().max(1.0) as usize,
        (size.y * ppp).round().max(1.0) as usize,
    ];
    match cache {
        Some((shown, shown_size, texture)) if shown == gradient && *shown_size == pixels => {
            texture.clone()
        }
        _ => {
            let rgba = gradient.render_sideways(pixels[0] as u32, pixels[1] as u32);
            let image = egui::ColorImage::from_rgba_unmultiplied(pixels, &rgba);
            let texture =
                ui.ctx()
                    .load_texture("gradient_bar", image, egui::TextureOptions::NEAREST);
            *cache = Some((gradient.clone(), pixels, texture.clone()));
            texture
        }
    }
}

/// Moves stop `index` to `at`, keeping the stops ordered; returns its new index.
fn move_stop(stops: &mut Vec<Stop>, index: usize, at: f64) -> usize {
    let mut stop = stops.remove(index);
    stop.at = at;
    // Ties keep their order: the stop goes after equal ones that were before it.
    let to = stops
        .iter()
        .enumerate()
        .filter(|(j, s)| s.at < at || (s.at == at && *j < index))
        .count();
    stops.insert(to, stop);
    to
}

/// Bar, stop handles and the selected stop's fields; see docs/features/gradients.md#gradient-editor.
fn gradient_editor(ui: &mut Ui, gradient: &mut Gradient, card: &mut GradientCard) {
    const REMOVE_DISTANCE: f32 = 36.0;
    let stops = &mut gradient.stops;
    card.selected = card.selected.min(stops.len() - 1);
    let width = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
    let (track, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
    let response = ui.interact(
        bar.union(track),
        ui.id().with("stops"),
        Sense::click_and_drag(),
    );
    let x_of = |at: f64| bar.left() + at as f32 * bar.width();
    let at_of = |x: f32| {
        let at = f64::from(((x - bar.left()) / bar.width()).clamp(0.0, 1.0));
        (at * 10_000.0).round() / 10_000.0
    };
    // The handle under a point: the nearest within half a handle, selected first.
    let handle_at = |stops: &[Stop], p: egui::Pos2| {
        if !track.expand(2.0).contains(p) {
            return None;
        }
        (0..stops.len())
            .filter(|&i| (x_of(stops[i].at) - p.x).abs() <= 7.0)
            .min_by(|&a, &b| {
                let d = |i: usize| (x_of(stops[i].at) - p.x).abs();
                d(a).total_cmp(&d(b))
            })
    };
    let (now, pressed, down, pointer) = ui.input(|i| {
        (
            i.time,
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.latest_pos(),
        )
    });

    if pressed
        && response.hovered()
        && let Some(p) = pointer
    {
        let index = match handle_at(stops, p) {
            Some(index) => index,
            None => {
                // Added in the color the gradient has there, and dragged on.
                let at = at_of(p.x);
                let color = gradient_color_at(stops, at);
                let index = stops.iter().filter(|s| s.at <= at).count();
                stops.insert(index, Stop { at, color });
                card.added_at = now;
                index
            }
        };
        card.selected = index;
        card.drag = Some(StopDrag {
            index,
            start: p.x,
            offset: x_of(stops[index].at) - p.x,
            moved: false,
            removing: false,
        });
    }
    if let Some(drag) = &mut card.drag {
        if let Some(p) = pointer {
            drag.moved |= (p.x - drag.start).abs() >= 1.0;
            if drag.moved {
                drag.index = move_stop(stops, drag.index, at_of(p.x + drag.offset));
                card.selected = drag.index;
            }
            drag.removing = stops.len() > 2 && p.y > track.bottom() + REMOVE_DISTANCE;
        }
        if !down {
            if drag.removing {
                stops.remove(drag.index);
                card.selected = drag.index.saturating_sub(1);
            }
            card.drag = None;
        }
    }
    if response.double_clicked()
        && now - card.added_at > 0.5
        && stops.len() > 2
        && let Some(i) = pointer.and_then(|p| handle_at(stops, p))
    {
        stops.remove(i);
        card.selected = i.saturating_sub(1);
        card.drag = None;
    }

    // Bar: the gradient as rendered, at its size in screen pixels.
    color_picker::checker(ui, bar, 4);
    let texture = gradient_texture(ui, &mut card.bar, gradient, bar.size());
    egui::Image::from_texture((texture.id(), bar.size()))
        .corner_radius(4)
        .paint_at(ui, bar);

    // Handles, the selected one last so it is on top.
    let stops = &mut gradient.stops;
    let mut order: Vec<usize> = (0..stops.len()).filter(|&i| i != card.selected).collect();
    order.push(card.selected);
    for i in order {
        let stop = stops[i];
        let selected = i == card.selected;
        let removing = card
            .drag
            .as_ref()
            .is_some_and(|d| d.index == i && d.removing);
        let fade = if removing { 0.4 } else { 1.0 };
        let x = x_of(stop.at);
        let edge = if selected { ACCENT } else { TEXT_WEAK }.gamma_multiply(fade);
        let painter = ui.painter();
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos2(x, track.top() + 1.0),
                pos2(x + 4.0, track.top() + 6.0),
                pos2(x - 4.0, track.top() + 6.0),
            ],
            edge,
            Stroke::NONE,
        ));
        let square = Rect::from_min_size(pos2(x - 6.0, track.top() + 6.0), vec2(12.0, 12.0));
        if stop.color[3] < 255 && !removing {
            color_picker::checker(ui, square, 2);
        }
        let stroke = if selected {
            Stroke::new(2.0, edge)
        } else {
            Stroke::new(1.0, edge)
        };
        ui.painter().rect(
            square,
            2,
            color_picker::to_color32(stop.color).gamma_multiply(fade),
            stroke,
            egui::StrokeKind::Outside,
        );
    }
    let hovered_stop = pointer
        .filter(|_| card.drag.is_none())
        .and_then(|p| handle_at(stops, p));
    if card.drag.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if hovered_stop.is_some() && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if let Some(i) = hovered_stop
        && response.hovered()
    {
        let stop = stops[i];
        response.on_hover_text_at_pointer(format!(
            "{} @ {:.1}%",
            gradient::format_color(stop.color),
            stop.at * 100.0
        ));
    }
    ui.add_space(6.0);

    // The selected stop.
    let selected = card.selected;
    card.picker.show(ui, &mut stops[selected].color);
    ui.add_space(4.0);
    let mut percent = stops[selected].at * 100.0;
    prop_row(ui, "Position", |ui, width| {
        slider_box(ui, width, &mut percent, 0.0..=100.0, Some(1));
    });
    if percent != stops[selected].at * 100.0 {
        let at = (percent / 100.0 * 10_000.0).round() / 10_000.0;
        card.selected = move_stop(stops, selected, at.clamp(0.0, 1.0));
    }
    ui.horizontal(|ui| {
        if ui
            .button("Reverse")
            .on_hover_text("Mirror the stops")
            .clicked()
        {
            for stop in stops.iter_mut() {
                stop.at = ((1.0 - stop.at) * 10_000.0).round() / 10_000.0;
            }
            stops.reverse();
            card.selected = stops.len() - 1 - card.selected;
        }
        if ui
            .button("Distribute")
            .on_hover_text("Space the stops evenly")
            .clicked()
        {
            let last = (stops.len() - 1) as f64;
            for (i, stop) in stops.iter_mut().enumerate() {
                stop.at = (i as f64 / last * 10_000.0).round() / 10_000.0;
            }
        }
        let remove = egui::Button::image_and_text(Icon::Trash2.image(14.0, TEXT), "Remove stop");
        if ui
            .add_enabled(stops.len() > 2, remove)
            .on_disabled_hover_text("A gradient needs at least two stops")
            .clicked()
        {
            stops.remove(card.selected);
            card.selected = card.selected.saturating_sub(1);
        }
    });
}

/// The undithered color of `stops` at `at`.
fn gradient_color_at(stops: &[Stop], at: f64) -> [u8; 4] {
    Gradient {
        name: String::new(),
        dither: Dither::None,
        stops: stops.to_vec(),
        modifiers: Vec::new(),
        ..Gradient::default()
    }
    .color_at(at)
}

/// Drag-to-reorder for a column of cards (see docs/features/noise.md#modifiers-tab): draws the
/// dragged card's outline and the drop line, and on drop returns (from, to) for remove/insert.
fn card_reorder(
    ui: &Ui,
    cards: &[Rect],
    dragging: Option<usize>,
    dropped: Option<usize>,
) -> Option<(usize, usize)> {
    let from = dragging.or(dropped)?;
    let pointer = ui.ctx().pointer_latest_pos().map_or(0.0, |p| p.y);
    let to = cards.iter().filter(|r| r.center().y < pointer).count();
    let moves = to != from && to != from + 1;
    if dropped.is_some() {
        return moves.then_some((from, if to > from { to - 1 } else { to }));
    }
    let painter = ui.painter();
    painter.rect_stroke(
        cards[from],
        6,
        Stroke::new(1.0, ACCENT),
        egui::StrokeKind::Inside,
    );
    if moves {
        // In the middle of the 8 px gap above card `to` (or below the last).
        let y = cards
            .get(to)
            .map_or_else(|| cards[to - 1].bottom() + 4.0, |r| r.top() - 4.0);
        painter.hline(cards[from].x_range(), y, Stroke::new(2.0, ACCENT));
    }
    None
}

enum ModifierAction {
    Toggle(usize),
    Remove(usize),
}

/// An `.anoise` file's modifiers; see docs/features/noise.md#modifiers-tab.
fn modifiers_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Noise> {
    let edit = noise_edit(ui, inspected)?;
    modifier_list(ui, &mut edit.edit.modifiers);
    noise_commit(ui, edit)
}

/// An image's modifiers, kept in its sidecar; see docs/features/textures.md#modifiers.
fn image_modifiers_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Amod> {
    let edit = match &mut inspected.sidecar {
        Some(Ok(edit)) => edit,
        Some(Err(e)) => {
            placeholder(ui, e);
            return None;
        }
        None => return None,
    };
    modifier_list(ui, &mut edit.edit.modifiers);
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

/// An atlas' settings; returns them to write once nothing is being dragged or typed. See
/// docs/features/atlases.md#inspector.
fn atlas_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<crate::atlas::AtlasConfig> {
    use crate::atlas::Packing;
    let (edit, saved) = match &mut inspected.atlas {
        Some(Ok(pair)) => (&mut pair.0, &mut pair.1),
        Some(Err(e)) => {
            placeholder(ui, e);
            return None;
        }
        None => return None,
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                prop_row(ui, "Packing", |ui, width| {
                    egui::ComboBox::from_id_salt("atlas_packing")
                        .width(width - 8.0)
                        .selected_text(edit.packing.name())
                        .show_ui(ui, |ui| {
                            for p in Packing::ALL {
                                ui.selectable_value(&mut edit.packing, p, p.name());
                            }
                        });
                });
                let number = |ui: &mut Ui, label: &str, value: &mut u32, max: u32| {
                    prop_row(ui, label, |ui, width| {
                        let field = egui::DragValue::new(value).range(0..=max).speed(0.25);
                        ui.add_sized(vec2(width, 22.0), field);
                    });
                };
                if edit.packing == Packing::Grid {
                    number(ui, "Cell width", &mut edit.cell_width, 4096);
                    number(ui, "Cell height", &mut edit.cell_height, 4096);
                    number(ui, "Columns", &mut edit.columns, 1024);
                }
                number(ui, "Padding", &mut edit.padding, 64);
                let padding = edit.padding;
                number(ui, "Extrude", &mut edit.extrude, padding);
                edit.extrude = edit.extrude.min(edit.padding);
                if edit.packing == Packing::Pack {
                    prop_row(ui, "Trim", |ui, _| {
                        ui.checkbox(&mut edit.trim, "");
                    });
                }
                prop_row(ui, "Power of two", |ui, _| {
                    ui.checkbox(&mut edit.power_of_two, "");
                });
                prop_row(ui, "Max size", |ui, width| {
                    let field = egui::DragValue::new(&mut edit.max_size)
                        .range(1..=16384)
                        .speed(4.0);
                    ui.add_sized(vec2(width, 22.0), field);
                });
            });
        });
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit != saved).then(|| {
        *saved = edit.clone();
        edit.clone()
    })
}

/// The modifier cards and the Add button in a scrolling pane; see
/// docs/features/noise.md#modifiers-tab.
fn modifier_list(ui: &mut Ui, modifiers: &mut Vec<Modifier>) {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                modifier_cards(ui, modifiers);
            });
        });
}

/// The modifier cards and the Add button.
fn modifier_cards(ui: &mut Ui, modifiers: &mut Vec<Modifier>) {
    // Set from the header closures, applied after the list is drawn.
    let action = std::cell::Cell::new(None);
    let dragging = std::cell::Cell::new(None);
    let dropped = std::cell::Cell::new(None);
    let mut cards = Vec::with_capacity(modifiers.len());
    for (i, m) in modifiers.iter_mut().enumerate() {
        let enabled = m.enabled;
        let title = RichText::new(m.kind.name()).color(if enabled { TEXT } else { TEXT_WEAK });
        cards.push(prop_section(
            ui,
            ("modifier", i, m.kind.key()),
            Fold::Never,
            title,
            |ui| {
                let grip = drag_handle(ui);
                if grip.dragged() {
                    dragging.set(Some(i));
                }
                if grip.drag_stopped() {
                    dropped.set(Some(i));
                }
                let (icon, tip) = if enabled {
                    (Icon::Eye, "Disable")
                } else {
                    (Icon::EyeOff, "Enable")
                };
                if icon_button(ui, icon, tip, true).clicked() {
                    action.set(Some(ModifierAction::Toggle(i)));
                }
            },
            |ui, _| {
                if icon_button(ui, Icon::Trash2, "Remove", true).clicked() {
                    action.set(Some(ModifierAction::Remove(i)));
                }
            },
            |ui| {
                let specs = m.kind.specs();
                if specs.is_empty() {
                    ui.weak("No parameters");
                }
                for (spec, value) in specs.iter().zip(&mut m.params.0) {
                    param_row(ui, spec, value);
                }
            },
        ));
    }
    if let Some((from, to)) = card_reorder(ui, &cards, dragging.get(), dropped.get()) {
        let m = modifiers.remove(from);
        modifiers.insert(to, m);
    }
    match action.take() {
        Some(ModifierAction::Toggle(i)) => modifiers[i].enabled ^= true,
        Some(ModifierAction::Remove(i)) => {
            modifiers.remove(i);
        }
        None => {}
    }
    if modifiers.is_empty() {
        ui.weak("No modifiers");
        ui.add_space(6.0);
    }
    let button = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add modifier");
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        for kind in ModifierKind::ALL {
            if menu_item(ui, kind.name(), None).clicked() {
                modifiers.push(Modifier::new(kind));
                ui.close();
            }
        }
    });
}

fn info(ui: &mut Ui, inspection: &Inspection) {
    if inspection.missing {
        placeholder(ui, &format!("{} no longer exists", inspection.name));
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.label(RichText::new(&inspection.name).strong());
                ui.weak(&inspection.path);
                ui.add_space(6.0);
                egui::Grid::new("inspector_details")
                    .num_columns(2)
                    .spacing(vec2(12.0, 4.0))
                    .show(ui, |ui| {
                        for (label, value) in &inspection.rows {
                            ui.weak(*label);
                            // Wrap rather than widen the sidebar.
                            ui.add(egui::Label::new(value).wrap());
                            ui.end_row();
                        }
                    });
            });
        });
}

/// One card per animation, the unfolded one playing, with its events; returns the sidecar to
/// write. See docs/features/models.md#animations-tab.
fn animations_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    choice: &mut Option<usize>,
    playhead: f32,
) -> Option<Amod> {
    let animations = inspected.inspection.animations.clone();
    let mut edit = match &mut inspected.mesh {
        Some(Ok(edit)) => Some(edit),
        _ => None,
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let mut fold = None;
                for (a, (name, duration)) in animations.iter().enumerate() {
                    let count = edit.as_ref().map_or(0, |e| {
                        e.edit
                            .events
                            .iter()
                            .filter(|v| v.animation == *name)
                            .count()
                    });
                    let detail = match count {
                        0 => format!("{duration:.3} s"),
                        1 => format!("1 event · {duration:.3} s"),
                        n => format!("{n} events · {duration:.3} s"),
                    };
                    let (_, folded) = ui
                        .push_id(("animation", a), |ui| {
                            card(
                                ui,
                                Some(*choice == Some(a)),
                                RichText::new(name),
                                |_| {},
                                |ui, _| {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(&detail).size(13.0).color(TEXT_WEAK));
                                },
                                |ui| match &mut edit {
                                    Some(edit) => event_list(ui, edit, name, *duration, playhead),
                                    None => {
                                        ui.weak("No events");
                                    }
                                },
                            )
                        })
                        .inner;
                    if folded {
                        fold = Some(a);
                    }
                }
                if let Some(a) = fold {
                    *choice = (*choice != Some(a)).then_some(a);
                }
            });
        });
    let edit = edit?;
    mesh_commit(ui, edit)
}

/// An animation's events, by time, the chosen one's fields under its row; see
/// docs/features/animation-events.md#event-list.
fn event_list(ui: &mut Ui, edit: &mut MeshEdit, animation: &str, duration: f32, playhead: f32) {
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add event");
    if ui.add(add).clicked() {
        edit.edit.events.push(enrich::Event {
            animation: animation.to_owned(),
            name: "event".into(),
            time: (playhead.min(duration) * 1000.0).round() / 1000.0,
            properties: Properties::new(),
        });
        edit.event = Some(edit.edit.events.len() - 1);
    }
    ui.add_space(4.0);
    let mut rows: Vec<usize> = (0..edit.edit.events.len())
        .filter(|&i| edit.edit.events[i].animation == animation)
        .collect();
    rows.sort_by(|&a, &b| {
        let (ta, tb) = (edit.edit.events[a].time, edit.edit.events[b].time);
        ta.total_cmp(&tb).then(a.cmp(&b))
    });
    if rows.is_empty() {
        ui.weak("No events");
    }
    let mut delete = None;
    for i in rows {
        let chosen = edit.event == Some(i);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        let fill = if chosen {
            ACCENT.gamma_multiply(0.25)
        } else if response.hovered() {
            BG_HEADER
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 4, fill);
        let event = &mut edit.edit.events[i];
        ui.painter().text(
            pos2(rect.left() + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            format!("{:.3} s", event.time),
            FontId::monospace(12.0),
            TEXT_WEAK,
        );
        ui.painter().text(
            pos2(rect.left() + 76.0, rect.center().y),
            Align2::LEFT_CENTER,
            &event.name,
            FontId::proportional(14.0),
            TEXT,
        );
        // Delete at the row's right end, over the row so it takes its own clicks.
        let bin = Rect::from_min_max(pos2(rect.right() - 28.0, rect.top()), rect.max);
        let deleted = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(bin)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| icon_button(ui, Icon::Trash2, "Delete", true).clicked(),
            )
            .inner;
        if deleted {
            delete = Some(i);
            continue;
        }
        if response.clicked() {
            edit.event = (!chosen).then_some(i);
        }
        if !chosen {
            continue;
        }
        ui.push_id(("event", i), |ui| {
            prop_row(ui, "Name", |ui, width| {
                ui.add_sized(
                    vec2(width, 22.0),
                    egui::TextEdit::singleline(&mut event.name),
                );
            });
            prop_row(ui, "Time", |ui, width| {
                let field = egui::DragValue::new(&mut event.time)
                    .range(0.0..=duration)
                    .speed(0.01)
                    .fixed_decimals(3)
                    .suffix(" s");
                ui.add_sized(vec2(width, 22.0), field);
            });
            ui.add_space(4.0);
            ui.label(RichText::new("Properties").size(13.0).color(TEXT_WEAK));
            properties_editor(ui, &mut event.properties);
            ui.add_space(8.0);
        });
    }
    if let Some(i) = delete {
        edit.edit.events.remove(i);
        edit.event = None;
    }
}

/// None, then one row per color set of a compiled mesh, the chosen one shown by the canvas;
/// see docs/features/vertex-colors.md#compiled-meshes.
fn mesh_colors_tab(ui: &mut Ui, mesh: &crate::amesh::Preview, choice: &mut Option<usize>) {
    let rows: Vec<_> = std::iter::once(("None".to_owned(), String::new()))
        .chain(
            mesh.color_sets
                .iter()
                .map(|(name, source)| (name.clone(), crate::amesh::source_name(*source))),
        )
        .collect();
    // Row 0 is None, row `i + 1` set `i`.
    let chosen = choice.map_or(0, |i| i + 1);
    if let Some(row) = choice_list(ui, &rows, chosen) {
        *choice = row.checked_sub(1);
    }
}

/// One row per mip level, the chosen one shown by the canvas; see
/// docs/features/inspector.md#mipmaps-tab.
fn mipmaps_tab(ui: &mut Ui, levels: &[Image], choice: &mut usize) {
    let rows: Vec<_> = levels
        .iter()
        .enumerate()
        .map(|(i, level)| {
            (
                format!("Level {i}"),
                format!("{} × {}", level.width, level.height),
            )
        })
        .collect();
    if let Some(row) = choice_list(ui, &rows, *choice) {
        *choice = row;
    }
}

/// One row per level of detail, the chosen one drawn by the canvas; see
/// docs/features/models.md#levels-of-detail-tab.
fn lods_tab(ui: &mut Ui, mesh: &crate::amesh::Preview, choice: &mut usize) {
    let rows: Vec<_> = mesh
        .levels
        .iter()
        .enumerate()
        .map(|(i, level)| {
            let same = if i > 0 && level.same_as_previous {
                format!(" (same as LOD {})", i - 1)
            } else {
                String::new()
            };
            (
                format!("LOD {i}"),
                format!("{} triangles{same}", level.triangles()),
            )
        })
        .collect();
    if let Some(row) = choice_list(ui, &rows, *choice) {
        *choice = row;
    }
}

/// Full-width rows of (label, weak right-aligned detail), one chosen; returns the clicked row.
fn choice_list(ui: &mut Ui, rows: &[(String, String)], chosen: usize) -> Option<usize> {
    let mut clicked = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                clicked = choice_rows(ui, rows, Some(chosen));
            });
        });
    clicked
}

/// The rows of a [`choice_list`], where it is; returns the clicked row.
fn choice_rows(ui: &mut Ui, rows: &[(String, String)], chosen: Option<usize>) -> Option<usize> {
    let mut clicked = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (label, detail)) in rows.iter().enumerate() {
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
            let painter = ui.painter();
            if chosen == Some(i) {
                painter.rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.25));
            } else if response.hovered() {
                painter.rect_filled(rect, 0.0, BG_HEADER);
            }
            painter.text(
                pos2(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(14.0),
                TEXT,
            );
            painter.text(
                pos2(rect.right() - 10.0, rect.center().y),
                Align2::RIGHT_CENTER,
                detail,
                FontId::proportional(13.0),
                TEXT_WEAK,
            );
            if response.clicked() {
                clicked = Some(i);
            }
        }
    });
    clicked
}

/// `texture`: a compiled texture's type and usage, shown after its kind.
fn assets_tab(ui: &mut Ui, assets: &Assets, file: &std::path::Path, texture: Option<&str>) {
    match assets {
        Assets::NoProject | Assets::NotBuilt => return placeholder(ui, "Not built yet"),
        Assets::Failed(e) => return placeholder(ui, e),
        Assets::Loaded(_) => {}
    }
    let list = assets.in_file(file);
    if list.is_empty() {
        return placeholder(ui, "Not in the manifest");
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                for asset in list {
                    ui.horizontal(|ui| {
                        let icon = match asset.kind {
                            Kind::Texture => Icon::Image,
                            Kind::Material => Icon::File,
                            Kind::Mesh => Icon::Torus,
                            Kind::Animation => Icon::Film,
                            Kind::Font => Icon::Type,
                            Kind::Schema => Icon::Braces,
                            Kind::Data => Icon::Package,
                            Kind::Graph => Icon::GraphStar,
                            Kind::Tree => Icon::ListTree,
                            Kind::Atlas => Icon::Map,
                            Kind::Gradient => Icon::SwatchBook,
                            Kind::Palette => Icon::Palette,
                        };
                        ui.add(icon.image(16.0, TEXT_WEAK));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&asset.asset_path).strong());
                            ui.horizontal(|ui| {
                                match texture.filter(|_| asset.kind == Kind::Texture) {
                                    Some(t) => ui.weak(format!("{}, {t}", asset.kind.name())),
                                    None => ui.weak(asset.kind.name()),
                                };
                                ui.weak(format!("{:#018x}", asset.id));
                            });
                        });
                    });
                    ui.add_space(6.0);
                }
            });
        });
}

/// The view bar: `View` and a segmented choice of `options`, see docs/features/canvas.md#view-bar.
/// No options draws an empty bar. Returns the clicked option.
fn view_bar(
    ui: &mut Ui,
    bar: Rect,
    options: &[&str],
    active: Option<usize>,
    enabled: bool,
) -> Option<usize> {
    let painter = ui.painter_at(bar);
    painter.rect_filled(bar, 0.0, BG_PANEL);
    painter.hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, STROKE));
    if options.is_empty() {
        return None;
    }
    let font = FontId::proportional(13.0);
    let label = painter.text(
        pos2(bar.left() + 12.0, bar.center().y),
        Align2::LEFT_CENTER,
        "View",
        font.clone(),
        TEXT_WEAK,
    );

    const HEIGHT: f32 = 24.0;
    const RADIUS: u8 = 4;
    let widths: Vec<f32> = options
        .iter()
        .map(|name| {
            painter
                .layout_no_wrap((*name).into(), font.clone(), TEXT)
                .size()
                .x
                + 20.0
        })
        .collect();
    let group = Rect::from_min_size(
        pos2(label.right() + 12.0, bar.center().y - HEIGHT / 2.0),
        vec2(widths.iter().sum(), HEIGHT),
    );
    painter.rect_filled(group, RADIUS, BG_BAR);
    let mut x = group.left();
    let last = options.len() - 1;
    let mut clicked = None;
    for (i, (name, width)) in options.iter().zip(widths).enumerate() {
        let rect = Rect::from_min_size(pos2(x, group.top()), vec2(width, HEIGHT));
        x += width;
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui.interact(rect, ui.id().with(("view_mode", i)), sense);
        let outer = |edge: bool| if edge { RADIUS } else { 0 };
        let radius = egui::CornerRadius {
            nw: outer(i == 0),
            sw: outer(i == 0),
            ne: outer(i == last),
            se: outer(i == last),
        };
        let color = if active == Some(i) {
            painter.rect_filled(rect, radius, ACCENT);
            BG_BAR
        } else if !enabled {
            TEXT_WEAK.gamma_multiply(0.5)
        } else if response.hovered() {
            painter.rect_filled(rect, radius, BG_HEADER);
            TEXT
        } else {
            TEXT_WEAK
        };
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            *name,
            font.clone(),
            color,
        );
        if enabled && response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// What the canvas shows for the selection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shown {
    Image,
    /// A font's specimen sheet: an image without tiling, channels or 3D.
    Specimen,
    Environment,
    Model,
    /// An `.amesh.bin` from the Output tree, with its meshlets.
    CompiledMesh,
    /// An `.aanim.bin` from the Output tree, played on its companion mesh.
    CompiledAnimation,
    /// A compiled schema, data file, graph or tree, as its chunks.
    CompiledData,
    /// An `.afont.bin` from the Output tree: its atlas (like a specimen) or its chunks.
    CompiledFont,
    Lut,
    /// A file shown as a table of its data (manifest, `.amat.bin`).
    Data,
    /// An `.aschema`'s type editor.
    Schema,
    /// An `.agraph`'s grid of nodes and edges.
    Graph,
    /// An `.adata`'s instance editor.
    Instances,
    /// An `.amat` on a shape in 3D.
    Material,
    /// A `.pak`: its entries or how its bytes are used.
    Pack,
    Other,
}

impl Shown {
    fn of(inspected: Option<&Inspected>) -> Self {
        match inspected {
            Some(i) if i.material.is_some() => Self::Material,
            Some(i) if i.inspection.pack.is_some() => Self::Pack,
            Some(i) if i.cubemap.is_some() => Self::Environment,
            Some(i) if i.model.is_some() => Self::Model,
            Some(i) if i.inspection.compiled_animation.is_some() => Self::CompiledAnimation,
            Some(i) if i.inspection.compiled_data => Self::CompiledData,
            Some(i) if i.compiled_mesh.is_some() => Self::CompiledMesh,
            Some(i) if i.inspection.compiled_font => Self::CompiledFont,
            Some(i) if i.lut.is_some() => Self::Lut,
            Some(i) if i.inspection.specimen && i.preview.is_some() => Self::Specimen,
            Some(i) if i.preview.is_some() && i.pixels.is_some() => Self::Image,
            Some(i) if i.schema.is_some() => Self::Schema,
            Some(i) if i.graph.is_some() => Self::Graph,
            Some(i) if i.data.is_some() => Self::Instances,
            Some(i) if i.inspection.data.is_some() => Self::Data,
            _ => Self::Other,
        }
    }
}

/// The selection's image if it has one; otherwise draws the canvas message and returns `None`.
fn shown_image<'a>(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&'a mut Inspected>,
) -> Option<&'a mut Inspected> {
    let message = match inspected {
        None => "Select a file to preview".to_owned(),
        Some(i) if i.inspection.missing => format!("{} no longer exists", i.inspection.name),
        Some(i) if i.preview.is_none() || i.pixels.is_none() => i
            .inspection
            .note
            .clone()
            .unwrap_or_else(|| "No preview".into()),
        Some(i) => return Some(i),
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        message,
        FontId::proportional(14.0),
        TEXT_WEAK,
    );
    None
}

/// The image's toolbar state, with its channel choice replaced if the image lacks it.
fn image_options(
    inspected: &Inspected,
    all_options: &HashMap<PathBuf, CanvasOptions>,
) -> CanvasOptions {
    let mut options = all_options
        .get(&inspected.path)
        .copied()
        .unwrap_or_default();
    let choices = channel_choices(&inspected.inspection);
    if !choices.contains(&(options.channels, true)) {
        options.channels = choices[0].0;
    }
    options
}

/// Channel buttons for an image and whether each is enabled; see docs/features/canvas.md#toolbar.
fn channel_choices(inspection: &Inspection) -> Vec<(Channels, bool)> {
    let [r, g, b, a] = inspection.channels;
    if !(r || g || b) {
        let mut choices = vec![(Channels::Gray, true)];
        if a {
            choices.push((Channels::A, true));
        }
        return choices;
    }
    [
        Channels::Rgb,
        Channels::R,
        Channels::G,
        Channels::B,
        Channels::A,
    ]
    .map(|c| (c, c.index().is_none_or(|i| inspection.channels[i])))
    .to_vec()
}

fn channel_tools(choices: &[(Channels, bool)], current: Channels) -> Vec<Tool> {
    choices
        .iter()
        .map(|&(channels, enabled)| Tool {
            glyph: Glyph::Channels(channels),
            name: if enabled {
                channels.name().into()
            } else {
                format!("{} (not in this image)", channels.name())
            },
            on: current == channels,
            enabled,
        })
        .collect()
}

/// RGBA8 pixels of `image` as shown for `channels`: as stored, or one channel as opaque gray.
fn channel_pixels(image: &Image, channels: Channels) -> Vec<u8> {
    match channels.index() {
        None => image.pixels.clone(),
        Some(c) => image
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[c], p[c], p[c], 255])
            .collect(),
    }
}

/// 2D canvas: the selected file's image with zoom, pan and tiling, see docs/features/canvas.md#2d.
fn canvas(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&mut Inspected>,
    view: &mut View,
    all_options: &mut HashMap<PathBuf, CanvasOptions>,
    vision: &mut Option<Vision>,
) {
    let Some(inspected) = shown_image(ui, rect, inspected) else {
        return;
    };
    let options = image_options(inspected, all_options);
    let (Some(preview), Some(pixels)) = (&inspected.preview, &inspected.pixels) else {
        return;
    };
    let texture = match *vision {
        // As a color vision deficiency sees the shown channels.
        Some(seen) => {
            let key = (options.channels, seen);
            let cached = &mut inspected.vision_preview;
            if cached.as_ref().is_none_or(|(k, _)| *k != key) {
                let shown = if options.channels.index().is_none() {
                    seen.simulate(&pixels.pixels)
                } else {
                    seen.simulate(&channel_pixels(pixels, options.channels))
                };
                let texture = load_preview(ui.ctx(), "canvas_vision", pixels, &shown);
                *cached = Some((key, texture));
            }
            cached.as_ref().unwrap().1.clone()
        }
        None if options.channels.index().is_none() => preview.clone(),
        None => {
            let cached = &mut inspected.channel_preview;
            if cached.as_ref().is_none_or(|(c, _)| *c != options.channels) {
                let gray = channel_pixels(pixels, options.channels);
                let texture = load_preview(ui.ctx(), "canvas_channel", pixels, &gray);
                *cached = Some((options.channels, texture));
            }
            cached.as_ref().unwrap().1.clone()
        }
    };

    let ppp = ui.ctx().pixels_per_point();
    let tiles = if options.tiled { 3.0 } else { 1.0 };
    // Texels of everything shown: the image, or the 3 × 3 block.
    let texels = texture.size_vec2() * tiles;
    let fit = |view: &mut View| {
        let room = (rect.size() - Vec2::splat(2.0 * FIT_MARGIN)).max(Vec2::splat(1.0));
        *view = View {
            zoom: ((room / texels).min_elem() * ppp).clamp(MIN_ZOOM, MAX_ZOOM),
            pan: Vec2::ZERO,
            fit: true,
        };
    };
    if view.fit {
        fit(view);
    }

    let response = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
    if response.double_clicked() {
        fit(view);
    } else if response.dragged_by(PointerButton::Secondary) {
        view.pan += response.drag_delta();
        view.fit = false;
    }
    if response.hovered() {
        let (notches, pinch, pointer) =
            ui.input(|i| (wheel_notches(i), i.zoom_delta(), i.pointer.hover_pos()));
        let zoom = pinch * WHEEL_ZOOM.powf(notches);
        if zoom != 1.0
            && let Some(pointer) = pointer
        {
            zoom_at(view, pointer - rect.center(), zoom);
        }
    }

    // Keep at least 32 px (or all) of the image inside the canvas on each axis.
    let size = texels * view.zoom / ppp;
    let limit = (rect.size() + size) / 2.0 - size.min(Vec2::splat(32.0));
    view.pan = view.pan.clamp(-limit, limit);

    let shown = Rect::from_center_size(rect.center() + view.pan, size);
    let painter = ui.painter_at(rect);
    if options.channels.index().is_none() {
        checkerboard(&painter, shown);
    }
    // Tiles share edge coordinates so neighbours touch exactly.
    let n = tiles as usize;
    let edge = |axis: usize, i: usize| shown.min[axis] + shown.size()[axis] * i as f32 / tiles;
    for y in 0..n {
        for x in 0..n {
            let tile = Rect::from_min_max(
                pos2(edge(0, x), edge(1, y)),
                pos2(edge(0, x + 1), edge(1, y + 1)),
            );
            painter.image(
                texture.id(),
                tile,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
    // An atlas' frames, outlined; the one under the pointer named. See
    // docs/features/atlases.md#inspector.
    let frames = &inspected.inspection.atlas_frames;
    if !frames.is_empty() {
        let tile = Rect::from_min_max(shown.min, pos2(edge(0, 1), edge(1, 1)));
        let scale = tile.size() / texels;
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let mut named = None;
        for (i, (name, [x, y, w, h])) in frames.iter().enumerate() {
            let frame = Rect::from_min_size(
                tile.min + vec2(*x as f32, *y as f32) * scale,
                vec2(*w as f32, *h as f32) * scale,
            );
            painter.rect_stroke(
                frame,
                0,
                Stroke::new(1.0, ACCENT.gamma_multiply(0.6)),
                StrokeKind::Outside,
            );
            if pointer.is_some_and(|p| frame.contains(p)) {
                named = Some((frame, format!("{name} (#{i})")));
            }
        }
        if let Some((frame, text)) = named {
            let galley = painter.layout_no_wrap(text, FontId::proportional(12.0), TEXT);
            let at = frame.left_top() + vec2(0.0, -galley.size().y - 4.0);
            painter.rect_filled(
                Rect::from_min_size(at, galley.size()).expand(3.0),
                3,
                BG_BAR.gamma_multiply(0.85),
            );
            painter.galley(at, galley, TEXT);
        }
    }

    // Drawn last so they sit on top; their clicks apply next frame.
    if inspected.inspection.specimen {
        let seen = vision_tools(*vision);
        if let Some((_, button)) = toolbar(ui, rect, &[&seen]) {
            toggle_vision(vision, button);
            ui.ctx().request_repaint();
        }
        if zoom_bar(ui, rect, view, fit) {
            ui.ctx().request_repaint();
        }
        return;
    }
    let tile = [Tool {
        glyph: Glyph::Icon(Icon::Grid3x3),
        name: "Tile".into(),
        on: options.tiled,
        enabled: true,
    }];
    let choices = channel_choices(&inspected.inspection);
    let channels = channel_tools(&choices, options.channels);
    let seen = vision_tools(*vision);
    if let Some((group, button)) = toolbar(ui, rect, &[&tile, &channels, &seen]) {
        let options = all_options.entry(inspected.path.clone()).or_default();
        match group {
            0 => options.tiled = !options.tiled,
            1 => options.channels = choices[button].0,
            _ => toggle_vision(vision, button),
        }
        ui.ctx().request_repaint();
    }
    if zoom_bar(ui, rect, view, fit) {
        ui.ctx().request_repaint();
    }
}

/// 3D canvas: the image on a rotating shape rendered by [`preview3d`] under this
/// (transparent) area, see docs/features/canvas.md#3d.
fn canvas_3d(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&mut Inspected>,
    all_options: &mut HashMap<PathBuf, CanvasOptions>,
    preview: &mut Preview3d,
    vision: &mut Option<Vision>,
) {
    let Some(inspected) = shown_image(ui, rect, inspected) else {
        return;
    };
    let options = image_options(inspected, all_options);
    let Some(pixels) = &inspected.pixels else {
        return;
    };
    if inspected.uploaded_3d != Some(options.channels) {
        preview.upload = Some(Upload {
            width: pixels.width,
            height: pixels.height,
            pixels: channel_pixels(pixels, options.channels),
            blend: options.channels.index().is_none() && inspected.inspection.channels[3],
        });
        inspected.uploaded_3d = Some(options.channels);
    }
    view_3d(ui, rect, preview, true);

    let shapes = shape_tools(preview.shape);
    let choices = channel_choices(&inspected.inspection);
    let channels = channel_tools(&choices, options.channels);
    let seen = vision_tools(*vision);
    if let Some((group, button)) = toolbar(ui, rect, &[&shapes, &channels, &seen]) {
        match group {
            0 => preview.shape = Shape::ALL[button],
            1 => {
                let options = all_options.entry(inspected.path.clone()).or_default();
                options.channels = choices[button].0;
            }
            _ => toggle_vision(vision, button),
        }
        ui.ctx().request_repaint();
    }
}

/// LUT views, see docs/features/lut.md#views.
fn canvas_lut(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&mut Inspected>,
    lut_ui: &mut LutUi,
    last_image: Option<&LastImage>,
    preview: &mut Preview3d,
    vision: &mut Option<Vision>,
) {
    let Some(Inspected {
        lut: Some(lut),
        lut_compare,
        uploaded_color_cube,
        ..
    }) = inspected
    else {
        return;
    };
    match lut_ui.view {
        LutView::Compare => compare_view(ui, rect, lut, lut_compare, lut_ui, last_image, vision),
        LutView::Cube => {
            if !*uploaded_color_cube {
                let before = preview3d::color_cube(lut.size, |rgb| rgb);
                let after = preview3d::color_cube(lut.size, |rgb| lut.apply(rgb));
                preview.color_cube = Some((before, after));
                *uploaded_color_cube = true;
            }
            preview.colors = true;
            view_3d(ui, rect, preview, true);
            // Holding Space shows Before, whatever the toolbar says.
            let held =
                !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_down(egui::Key::Space));
            preview.colors_before = lut_ui.cube_before || held;
            let tools = [
                Tool {
                    glyph: Glyph::Icon(Icon::EyeOff),
                    name: "Before".into(),
                    on: lut_ui.cube_before,
                    enabled: true,
                },
                Tool {
                    glyph: Glyph::Icon(Icon::Eye),
                    name: "After".into(),
                    on: !lut_ui.cube_before,
                    enabled: true,
                },
            ];
            let seen = vision_tools(*vision);
            match toolbar(ui, rect, &[&tools, &seen]) {
                Some((0, button)) => lut_ui.cube_before = button == 0,
                Some((_, button)) => toggle_vision(vision, button),
                None => return,
            }
            ui.ctx().request_repaint();
        }
        LutView::Curves => curves_view(ui, rect, lut),
    }
}

/// `size` scaled to fit `rect` with the fit margin, centered.
fn fit_rect(rect: Rect, size: Vec2) -> Rect {
    let room = (rect.size() - Vec2::splat(2.0 * FIT_MARGIN)).max(Vec2::splat(1.0));
    Rect::from_center_size(rect.center(), size * (room / size).min_elem())
}

/// Reference image, original left and graded right of a draggable divider.
fn compare_view(
    ui: &mut Ui,
    rect: Rect,
    lut: &Lut,
    cache: &mut Option<CompareCache>,
    lut_ui: &mut LutUi,
    last_image: Option<&LastImage>,
    vision: &mut Option<Vision>,
) {
    let (key, image) = match (lut_ui.source, last_image) {
        (LutSource::Image, Some(last)) => (Some(&last.path), &last.image),
        _ => (None, &*chart::CHART),
    };
    if cache
        .as_ref()
        .is_none_or(|c| c.key.as_ref() != key || c.vision != *vision)
    {
        let graded = lut.grade(image);
        let linear = egui::TextureFilter::Linear;
        let seen = |pixels: &[u8]| match *vision {
            Some(seen) => seen.simulate(pixels),
            None => pixels.to_vec(),
        };
        *cache = Some(CompareCache {
            key: key.cloned(),
            vision: *vision,
            before: load_texture(ui.ctx(), "lut_before", image, &seen(&image.pixels), linear),
            after: load_texture(
                ui.ctx(),
                "lut_after",
                &graded,
                &seen(&graded.pixels),
                linear,
            ),
        });
    }
    let cache = cache.as_ref().unwrap();
    let shown = fit_rect(rect, cache.before.size_vec2());

    // Drag the grab handle or the line to move the divider.
    let x = shown.left() + shown.width() * lut_ui.split;
    let handle = Rect::from_center_size(
        pos2(
            x,
            shown
                .center()
                .y
                .clamp(rect.top() + 20.0, rect.bottom() - 20.0),
        ),
        vec2(24.0, 40.0),
    );
    let line = Rect::from_x_y_ranges(x - 4.0..=x + 4.0, shown.y_range());
    let id = ui.id().with("lut_split");
    let response = ui.interact(handle, id.with("handle"), Sense::drag())
        | ui.interact(line, id.with("line"), Sense::drag());
    if response.dragged_by(PointerButton::Primary) {
        let delta = response.drag_delta().x / shown.width();
        lut_ui.split = (lut_ui.split + delta).clamp(0.0, 1.0);
    }
    let active = response.hovered() || response.dragged();
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    let x = shown.left() + shown.width() * lut_ui.split;
    let handle = handle.translate(vec2(x - handle.center().x, 0.0));
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let painter = ui.painter_at(rect);
    let left = Rect::from_min_max(shown.min, pos2(x, shown.bottom()));
    let right = Rect::from_min_max(pos2(x, shown.top()), shown.max);
    let white = egui::Color32::WHITE;
    painter
        .with_clip_rect(left.intersect(rect))
        .image(cache.before.id(), shown, uv, white);
    painter
        .with_clip_rect(right.intersect(rect))
        .image(cache.after.id(), shown, uv, white);
    painter.vline(x, shown.y_range(), Stroke::new(2.0, TEXT));
    let tint = if active { ACCENT } else { TEXT };
    let border = if active { ACCENT } else { STROKE };
    painter.rect(
        handle,
        12.0,
        BG_BAR,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    Icon::GripVertical.image(16.0, tint).paint_at(
        ui,
        Rect::from_center_size(handle.center(), vec2(16.0, 16.0)),
    );
    let pill = |text: &str, anchor: egui::Pos2, align: Align2| {
        let galley = painter.layout_no_wrap(text.into(), FontId::proportional(12.0), TEXT);
        let body = align.anchor_size(anchor, galley.size() + vec2(12.0, 6.0));
        painter.rect_filled(body, 4.0, BG_BAR);
        painter.galley(body.min + vec2(6.0, 3.0), galley, TEXT);
    };
    pill(
        "Before",
        pos2(x - 8.0, shown.top() + 8.0),
        Align2::RIGHT_TOP,
    );
    pill("After", pos2(x + 8.0, shown.top() + 8.0), Align2::LEFT_TOP);

    let sources = [
        Tool {
            glyph: Glyph::Icon(Icon::Palette),
            name: "Chart".into(),
            on: key.is_none(),
            enabled: true,
        },
        Tool {
            glyph: Glyph::Icon(Icon::Image),
            name: match last_image {
                Some(last) => format!("Image ({})", last.name),
                None => "Image (no image selected yet)".into(),
            },
            on: key.is_some(),
            enabled: last_image.is_some(),
        },
    ];
    let seen = vision_tools(*vision);
    match toolbar(ui, rect, &[&sources, &seen]) {
        Some((0, button)) => lut_ui.source = [LutSource::Chart, LutSource::Image][button],
        Some((_, button)) => toggle_vision(vision, button),
        None => return,
    }
    ui.ctx().request_repaint();
}

/// Output R, G, B for gray input 0..1; see docs/features/lut.md#curves.
fn curves_view(ui: &mut Ui, rect: Rect, lut: &Lut) {
    let side = (rect.size().min_elem() - 96.0).max(32.0);
    let plot = Rect::from_center_size(rect.center(), Vec2::splat(side));
    let painter = ui.painter_at(rect);
    painter.rect_filled(plot, 0.0, BG_PANEL);
    let grid = Stroke::new(1.0, STROKE);
    for i in 1..4 {
        let t = i as f32 / 4.0;
        painter.vline(plot.left() + side * t, plot.y_range(), grid);
        painter.hline(plot.x_range(), plot.bottom() - side * t, grid);
    }
    painter.rect_stroke(plot, 0.0, grid, StrokeKind::Inside);
    painter.line_segment(
        [plot.left_bottom(), plot.right_top()],
        Stroke::new(1.0, TEXT_WEAK),
    );
    for (channel, color) in [CHANNEL_R, CHANNEL_G, CHANNEL_B].into_iter().enumerate() {
        let points = (0..256)
            .map(|i| {
                let x = i as f32 / 255.0;
                let y = lut.apply([x; 3])[channel];
                pos2(plot.left() + x * side, plot.bottom() - y * side)
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(2.0, color)));
    }
    let label = |text: &str, pos: egui::Pos2, align: Align2| {
        painter.text(pos, align, text, FontId::proportional(12.0), TEXT_WEAK);
    };
    label("0", plot.left_bottom() + vec2(-6.0, 4.0), Align2::RIGHT_TOP);
    label(
        "1",
        plot.right_bottom() + vec2(0.0, 4.0),
        Align2::CENTER_TOP,
    );
    label("1", plot.left_top() + vec2(-6.0, 0.0), Align2::RIGHT_CENTER);
    label(
        "Input",
        plot.center_bottom() + vec2(0.0, 4.0),
        Align2::CENTER_TOP,
    );
    label(
        "Output",
        plot.left_center() + vec2(-6.0, 0.0),
        Align2::RIGHT_CENTER,
    );
}

/// 3D canvas for an environment map: the chosen shape lit by it, with it as the background.
/// See docs/features/canvas.md#environment-maps.
fn canvas_environment(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&mut Inspected>,
    preview: &mut Preview3d,
    vision: &mut Option<Vision>,
) {
    let Some(inspected) = inspected else {
        return;
    };
    let Some(faces) = &inspected.cubemap else {
        return;
    };
    if !inspected.uploaded_cubemap {
        preview.cubemap = Some(CubemapUpload {
            size: faces[0].width,
            pixels: faces
                .iter()
                .flat_map(|f| f.pixels.iter().copied())
                .collect(),
        });
        inspected.uploaded_cubemap = true;
    }
    preview.environment = true;
    view_3d(ui, rect, preview, true);
    let shapes = shape_tools(preview.environment_shape());
    let seen = vision_tools(*vision);
    match toolbar(ui, rect, &[&shapes, &seen]) {
        Some((0, button)) => preview.environment_shape = Some(Shape::ALL[button]),
        Some((_, button)) => toggle_vision(vision, button),
        None => return,
    }
    ui.ctx().request_repaint();
}

/// A compiled mesh in the 3D view in one of its display modes; see
/// docs/features/models.md#compiled-mesh-preview.
fn canvas_compiled_mesh(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&mut Inspected>,
    preview: &mut Preview3d,
    mesh_view: &mut MeshView,
    (lod, color_set): (usize, Option<usize>),
    vision: &mut Option<Vision>,
) {
    let Some(inspected) = inspected else {
        return;
    };
    let Some(mesh) = &inspected.compiled_mesh else {
        return;
    };
    let lod = lod.min(mesh.levels.len().saturating_sub(1));
    let color_set = color_set.filter(|&s| s < mesh.color_sets.len());
    if inspected.uploaded_lod != Some((lod, color_set))
        && let Some(level) = mesh.levels.get(lod)
    {
        // Every level is fitted by the full mesh's bounds.
        let (mut min, mut max) = (bevy::math::Vec3::INFINITY, bevy::math::Vec3::NEG_INFINITY);
        for p in &mesh.levels[0].positions {
            let p = bevy::math::Vec3::from(*p);
            (min, max) = (min.min(p), max.max(p));
        }
        preview.compiled = Some(CompiledUpload {
            level: level.clone(),
            color_set,
            bounds: [min, max],
            materials: inspected.compiled_materials.clone(),
            mesh_bounds: mesh.bounds,
            submesh_bounds: mesh.submesh_bounds.clone(),
            meshlet_bounds: mesh.meshlet_bounds.clone(),
            meshlet_boxes: mesh.meshlet_boxes(),
        });
        inspected.uploaded_lod = Some((lod, color_set));
    }
    preview.compiled_shown = true;
    let MeshView {
        display,
        overlays,
        hide_locators,
    } = mesh_view;
    preview.display = *display;
    // Meshlets exist only for LOD 0.
    let available = |overlay: MeshOverlay| lod == 0 || !overlay.of_meshlets();
    preview.overlays = std::array::from_fn(|i| overlays[i] && available(MeshOverlay::ALL[i]));
    view_3d(ui, rect, preview, true);
    let tools: Vec<Tool> = MeshDisplay::ALL
        .into_iter()
        .map(|mode| Tool {
            glyph: Glyph::Icon(match mode {
                MeshDisplay::Material => Icon::BoxChecker,
                MeshDisplay::Wireframe => Icon::Box,
                MeshDisplay::Shaded => Icon::BoxShaded,
                MeshDisplay::Meshlets => Icon::SquaresUnite,
                MeshDisplay::MaterialId => Icon::Image,
                MeshDisplay::Submeshes => Icon::Boxes,
            }),
            name: mode.name().into(),
            on: *display == mode,
            enabled: true,
        })
        .collect();
    let has_locators = !mesh.locators.is_empty();
    if has_locators && !*hide_locators {
        // Centered and scaled like the mesh, see docs/features/models.md#compiled-mesh-preview.
        use bevy::math::{Affine3A, Vec3};
        let (min, max) = (Vec3::from(mesh.bounds.min), Vec3::from(mesh.bounds.max));
        let size = (max - min).max_element();
        let scale = if size > 0.0 {
            preview3d::MODEL_SIZE / size
        } else {
            1.0
        };
        let fit = Affine3A::from_scale(Vec3::splat(scale))
            * Affine3A::from_translation(-(min + max) / 2.0);
        let camera = gizmo::ViewCamera::new(rect, &preview.orbit);
        let painter = ui.painter_at(rect);
        for (name, transform) in &mesh.locators {
            if let Some((dot, ends)) = gizmo::marker(&camera, fit * *transform) {
                paint_marker(&painter, dot, &ends, name, false, TEXT);
            }
        }
    }
    let mut bounds: Vec<Tool> = MeshOverlay::ALL
        .into_iter()
        .enumerate()
        .map(|(i, overlay)| Tool {
            glyph: Glyph::Icon(match overlay {
                MeshOverlay::BoundingBox => Icon::Scan,
                MeshOverlay::BoundingSphere => Icon::CircleDashed,
                MeshOverlay::SubmeshBoxes => Icon::Group,
                MeshOverlay::MeshletBoxes => Icon::LayoutGrid,
                MeshOverlay::MeshletSpheres => Icon::CircleDot,
                MeshOverlay::MeshletCones => Icon::Cone,
            }),
            name: overlay.name().into(),
            on: preview.overlays[i],
            enabled: available(overlay),
        })
        .collect();
    bounds.push(Tool {
        glyph: Glyph::Icon(Icon::MapPin),
        name: "Locators".into(),
        on: has_locators && !*hide_locators,
        enabled: has_locators,
    });
    let seen = vision_tools(*vision);
    match toolbar(ui, rect, &[&tools, &bounds, &seen]) {
        Some((0, button)) => {
            *display = MeshDisplay::ALL[button];
            ui.ctx().request_repaint();
        }
        Some((1, button)) if button == MeshOverlay::ALL.len() => *hide_locators = !*hide_locators,
        Some((1, button)) => {
            overlays[button] = !overlays[button];
            ui.ctx().request_repaint();
        }
        Some((_, button)) => {
            toggle_vision(vision, button);
            ui.ctx().request_repaint();
        }
        None => {}
    }
    // The level drawn, left of the toolbar's first button.
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(format!("LOD {lod}"), FontId::proportional(13.0), TEXT);
    let toolbar_left = rect.right() - 8.0 - 36.0;
    painter.galley(
        pos2(
            toolbar_left - 8.0 - galley.size().x,
            rect.top() + 8.0 + 16.0 - galley.size().y / 2.0,
        ),
        galley,
        TEXT,
    );
}

/// A file's data as a table; see docs/features/inspector.md#data-tables.
fn data_view(ui: &mut Ui, rect: Rect, table: &inspect::DataTable, filter: &mut String) {
    use egui_extras::{Column, TableBuilder};
    let area = rect.shrink(10.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&table.header)
                    .monospace()
                    .size(13.0)
                    .color(TEXT),
            );
            if table.filterable {
                ui.add_space(12.0);
                ui.add(
                    egui::TextEdit::singleline(filter)
                        .hint_text("Filter path…")
                        .desired_width(240.0),
                );
            }
        });
        ui.add_space(6.0);
        let needle = filter.to_lowercase();
        let rows: Vec<&inspect::DataRow> = table
            .rows
            .iter()
            .filter(|r| !table.filterable || needle.is_empty() || r.filter.contains(&needle))
            .collect();
        let text = |s: &str| RichText::new(s).monospace().size(12.0);
        egui::ScrollArea::horizontal()
            .id_salt("data_table_wide")
            .auto_shrink(false)
            .show(ui, |ui| {
                let mut builder = TableBuilder::new(ui)
                    .id_salt(("data_table", &table.header))
                    .striped(true)
                    .resizable(true)
                    .auto_shrink(false)
                    .cell_layout(Layout::left_to_right(Align::Center));
                for _ in 1..table.columns.len() {
                    builder = builder.column(Column::auto().at_least(40.0));
                }
                builder
                    .column(Column::remainder().at_least(40.0))
                    .header(20.0, |mut header| {
                        for name in &table.columns {
                            header.col(|ui| {
                                ui.label(text(name).color(TEXT_WEAK));
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(20.0, rows.len(), |mut row| {
                            let data = rows[row.index()];
                            let color = if data.weak { TEXT_WEAK } else { TEXT };
                            for (column, cell) in data.cells.iter().enumerate() {
                                row.col(|ui| {
                                    if let Some((at, [r, g, b])) = data.swatch
                                        && at == column
                                    {
                                        let (swatch, _) = ui
                                            .allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                        ui.painter().rect_filled(
                                            swatch,
                                            2,
                                            egui::Color32::from_rgb(r, g, b),
                                        );
                                        ui.painter().rect_stroke(
                                            swatch,
                                            2,
                                            Stroke::new(1.0, STROKE),
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    ui.add(egui::Label::new(text(cell).color(color)).extend());
                                });
                            }
                        });
                    });
            });
    });
}

/// How the Chunks view reads one chunked format.
struct ChunkSchema {
    describe: fn(&str) -> &'static str,
    table: fn(&[u8], &str) -> Option<crate::amesh::Table>,
    cell: fn(&[u8], &str, usize, usize) -> String,
}

/// The format a chunked file names in its header, and how to read it.
fn chunk_schema(bytes: &[u8]) -> Option<(&'static str, ChunkSchema)> {
    let name = bytes.get(..8)?;
    let is = |format: &str| {
        name.starts_with(format.as_bytes()) && name[format.len()..].iter().all(|&b| b == 0)
    };
    if is(crate::amesh::FORMAT) {
        Some((
            crate::amesh::FORMAT,
            ChunkSchema {
                describe: crate::amesh::describe,
                table: crate::amesh::table,
                cell: crate::amesh::cell,
            },
        ))
    } else if let Some(format) = crate::compiled::format_of(bytes) {
        Some((
            format,
            ChunkSchema {
                describe: crate::compiled::describe,
                table: crate::compiled::table,
                cell: crate::compiled::cell,
            },
        ))
    } else if is(crate::font::FORMAT) {
        Some((
            crate::font::FORMAT,
            ChunkSchema {
                describe: crate::font::describe,
                table: crate::font::table,
                cell: crate::font::cell,
            },
        ))
    } else {
        None
    }
}

/// A chunked file's chunks (a compiled mesh or font): a list, and the chosen one's table; see
/// docs/features/models.md#chunks-view.
fn chunks_view(ui: &mut Ui, rect: Rect, bytes: &[u8], chosen: &mut String) {
    let Some((format, schema)) = chunk_schema(bytes) else {
        return;
    };
    let Ok(reader) = crate::chunks::Reader::parse(bytes, format) else {
        return;
    };
    let entries = reader.entries();
    if !entries.iter().any(|e| e.name == *chosen)
        && let Some(first) = entries.first()
    {
        *chosen = first.name.clone();
    }
    let list = Rect::from_min_size(rect.min, vec2(220.0, rect.height()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(list, 0.0, BG_PANEL);
    painter.vline(list.right() - 0.5, list.y_range(), Stroke::new(1.0, STROKE));
    ui.scope_builder(egui::UiBuilder::new().max_rect(list), |ui| {
        egui::ScrollArea::vertical()
            .id_salt("chunk_list")
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for entry in &entries {
                    let (row, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                    let painter = ui.painter();
                    if entry.name == *chosen {
                        painter.rect_filled(row, 0.0, ACCENT.gamma_multiply(0.25));
                    } else if response.hovered() {
                        painter.rect_filled(row, 0.0, BG_HEADER);
                    }
                    painter.text(
                        pos2(row.left() + 10.0, row.top() + 12.0),
                        Align2::LEFT_CENTER,
                        &entry.name,
                        FontId::monospace(13.0),
                        TEXT,
                    );
                    painter.text(
                        pos2(row.left() + 10.0, row.top() + 29.0),
                        Align2::LEFT_CENTER,
                        (schema.describe)(&entry.name),
                        FontId::proportional(12.0),
                        TEXT_WEAK,
                    );
                    painter.text(
                        pos2(row.right() - 10.0, row.top() + 12.0),
                        Align2::RIGHT_CENTER,
                        inspect::format_size(entry.size),
                        FontId::proportional(12.0),
                        TEXT_WEAK,
                    );
                    if response.clicked() {
                        *chosen = entry.name.clone();
                    }
                }
            });
    });

    let Some(entry) = entries.iter().find(|e| e.name == *chosen) else {
        return;
    };
    let Some(table) = (schema.table)(bytes, &entry.name) else {
        return;
    };
    let area = Rect::from_min_max(pos2(list.right(), rect.top()), rect.max).shrink(10.0);
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        ui.label(
            RichText::new(format!(
                "{} — {}   @{}   {} bytes",
                entry.name,
                (schema.describe)(&entry.name),
                entry.offset,
                entry.size
            ))
            .monospace()
            .size(13.0)
            .color(TEXT),
        );
        ui.add_space(6.0);
        egui::ScrollArea::horizontal()
            .id_salt("chunk_table_wide")
            .auto_shrink(false)
            .show(ui, |ui| chunk_table(ui, bytes, entry, &table, schema.cell));
    });
}

/// The rows of one chunk; see docs/features/models.md#chunks-view.
fn chunk_table(
    ui: &mut Ui,
    bytes: &[u8],
    entry: &crate::chunks::Entry,
    table: &crate::amesh::Table,
    cell: fn(&[u8], &str, usize, usize) -> String,
) {
    use egui_extras::{Column, TableBuilder};
    let mut columns: Vec<&str> = Vec::new();
    if table.numbered {
        columns.push("#");
    }
    columns.extend(table.columns.iter().map(String::as_str));
    let text = |s: String| RichText::new(s).monospace().size(12.0);
    let mut builder = TableBuilder::new(ui)
        .id_salt(("chunk_table", &entry.name))
        .striped(true)
        .resizable(true)
        .auto_shrink(false)
        .cell_layout(Layout::left_to_right(Align::Center));
    for _ in 1..columns.len() {
        builder = builder.column(Column::auto().at_least(40.0));
    }
    builder
        .column(Column::remainder().at_least(40.0))
        .header(20.0, |mut header| {
            for name in &columns {
                header.col(|ui| {
                    ui.label(text((*name).to_owned()).color(TEXT_WEAK));
                });
            }
        })
        .body(|body| {
            body.rows(20.0, table.rows, |mut row| {
                let index = row.index();
                for column in 0..columns.len() {
                    row.col(|ui| {
                        let value = cell(bytes, &entry.name, index, column);
                        ui.add(egui::Label::new(text(value).color(TEXT)).extend());
                    });
                }
            });
        });
}

/// A glTF model in the 3D view, see docs/features/models.md#preview.
/// What the model canvas asks of the shell.
#[derive(Default)]
struct ModelCanvas {
    /// The sidecar to write.
    save: Option<Amod>,
    /// Switch the Inspector to this tab.
    show_tab: Option<InspectorTab>,
}

/// A model in 3D with its locators; see docs/features/models.md#preview and
/// docs/features/locators.md#3d-view.
fn canvas_model(
    ui: &mut Ui,
    rect: Rect,
    inspected: Option<&mut Inspected>,
    preview: &mut Preview3d,
    vision: &mut Option<Vision>,
    tools: &mut LocatorTools,
    tab: InspectorTab,
) -> ModelCanvas {
    let shapes_active = tab == InspectorTab::Shapes;
    let paths_active = tab == InspectorTab::Paths;
    let colors_active = tab == InspectorTab::VertexColors;
    use bevy::math::{Affine3A, Quat, Vec3};
    let mut out = ModelCanvas::default();
    let Some(inspected) = inspected else {
        return out;
    };
    let Some(model) = inspected.model.clone() else {
        return out;
    };
    preview.model = Some(model);
    let joints = &inspected.inspection.joints;
    let empty = HashSet::new();
    // Using a kind shows it; see docs/features/models.md#show.
    if tools.armed {
        tools.show[0] = true;
    }
    if tools.paint {
        tools.show[2] = true;
    }
    if tools.draw {
        tools.show[3] = true;
    }
    let [
        show_locators,
        show_shapes,
        show_regions,
        show_paths,
        show_grid,
    ] = tools.show;
    preview.grid = show_grid;
    let hidden = tools.hidden.get(&inspected.path).unwrap_or(&empty).clone();
    let hidden_shapes = tools
        .hidden_shapes
        .get(&inspected.path)
        .unwrap_or(&empty)
        .clone();
    let hidden_regions = tools
        .hidden_regions
        .get(&inspected.path)
        .unwrap_or(&empty)
        .clone();
    let hidden_paths = tools
        .hidden_paths
        .get(&inspected.path)
        .unwrap_or(&empty)
        .clone();
    let geometry = inspected.geometry.as_ref();
    let hulls = &mut inspected.hulls;
    let mut edit = match &mut inspected.mesh {
        Some(Ok(edit)) => Some(edit),
        _ => None,
    };
    // Transforms are relative to joints at rest; see docs/features/locators.md#animations.
    let locked = preview.animation.is_some();
    if locked {
        tools.armed = false;
    }
    if locked || !paths_active {
        tools.draw = false;
    }
    if tab != InspectorTab::Regions {
        tools.paint = false;
    }
    let painting = tools.paint && edit.as_ref().is_some_and(|e| e.region.is_some());
    let drawing = tools.draw && edit.as_ref().is_some_and(|e| e.path.is_some());
    let camera = gizmo::ViewCamera::new(rect, &preview.orbit);
    let frames = preview.frames.clone();

    // The answer to a ray: a stroke of paint, or a new locator; see
    // docs/features/regions.md#painting and docs/features/locators.md#place-on-surface.
    if let Some(hit) = preview.surface_hit.take() {
        let paint = std::mem::take(&mut tools.paint_awaiting);
        let place = std::mem::take(&mut tools.awaiting);
        let draw = std::mem::take(&mut tools.draw_awaiting);
        if draw {
            // A point where the click hit, after the chosen one; see
            // docs/features/paths.md#paths-tab.
            if let (Some(hit), Some(edit), Some(frames)) = (&hit, &mut edit, &frames)
                && let Some(p) = edit.path
                && let Some(path) = edit.edit.paths.get_mut(p)
                && let Some(parent) = frames.parent(path.joint.as_deref())
            {
                let at = edit
                    .point
                    .map_or(path.points.len(), |i| (i + 1).min(path.points.len()));
                let local = parent.inverse().transform_point3(hit.point);
                path.points.insert(at, local.to_array());
                edit.point = Some(at);
            }
        } else if paint
            && let (Some(hit), Some(edit)) = (&hit, &mut edit)
            && let Some(region) = edit.region.and_then(|r| edit.edit.regions.get_mut(r))
        {
            for id in &hit.brush {
                if tools.erase {
                    region.triangles.remove(id);
                } else {
                    region.triangles.insert(*id);
                }
            }
        } else if place
            && let (Some(hit), Some(edit), Some(frames)) = (hit, &mut edit, &frames)
            && !locked
        {
            add_locator(edit);
            let i = edit.edit.locators.len() - 1;
            let locator = &mut edit.edit.locators[i];
            locator.joint = hit.joint.filter(|j| joints.contains(j));
            if let Some(parent) = frames.parent(locator.joint.as_deref()) {
                gizmo::place(locator, parent, hit.point, hit.normal);
            }
            tools.armed = false;
            out.show_tab = Some(InspectorTab::Locators);
        }
    }

    // Each visible locator's world transform in the pose shown; `None` when hidden or its joint
    // is not found.
    let worlds: Vec<Option<Affine3A>> = match (&edit, &frames) {
        (Some(edit), Some(frames)) => edit
            .edit
            .locators
            .iter()
            .map(|l| {
                let parent = frames.parent(l.joint.as_deref())?;
                (show_locators && !hidden.contains(&l.name)).then(|| parent * gizmo::local(l))
            })
            .collect(),
        _ => Vec::new(),
    };
    // The same for shapes (without scale); hulls are in their parent's space.
    let shape_local = |s: &enrich::Shape| {
        let rotation = Quat::from_array(s.rotation);
        let rotation = if rotation.length_squared() > 0.0 {
            rotation.normalize()
        } else {
            Quat::IDENTITY
        };
        Affine3A::from_rotation_translation(rotation, Vec3::from(s.translation))
    };
    let shape_worlds: Vec<Option<Affine3A>> = match (&edit, &frames) {
        (Some(edit), Some(frames)) => edit
            .edit
            .shapes
            .iter()
            .map(|s| {
                let parent = frames.parent(s.joint.as_deref())?;
                let local = if s.shape == ShapeType::Hull {
                    Affine3A::IDENTITY
                } else {
                    shape_local(s)
                };
                (show_shapes && !hidden_shapes.contains(&s.name)).then(|| parent * local)
            })
            .collect(),
        _ => Vec::new(),
    };
    // Each visible path's parent; see docs/features/paths.md#3d-view.
    let path_parents: Vec<Option<Affine3A>> = match (&edit, &frames) {
        (Some(edit), Some(frames)) => edit
            .edit
            .paths
            .iter()
            .map(|p| {
                let parent = frames.parent(p.joint.as_deref())?;
                (show_paths && !hidden_paths.contains(&p.name)).then_some(parent)
            })
            .collect(),
        _ => Vec::new(),
    };
    let chosen_point = edit.as_ref().and_then(|e| {
        let p = e.path?;
        let parent = path_parents.get(p).copied().flatten()?;
        let point = e.edit.paths[p].points.get(e.point?)?;
        Some(parent * Affine3A::from_translation(Vec3::from(*point)))
    });
    // The chosen gradient's ends, in the model's space as posed; see
    // docs/features/vertex-colors.md#3d-view.
    let model_space = frames.as_ref().and_then(|f| f.parent(None));
    let model_colors: Vec<u32> = geometry.map(|g| g.color_sets.clone()).unwrap_or_default();
    let color_sets = edit
        .as_ref()
        .map(|e| e.color_sets(&model_colors))
        .unwrap_or_default();
    let gradient = match (&edit, model_space) {
        (Some(e), Some(model)) if colors_active => e
            .color
            .and_then(|i| color_sets.get(i))
            .filter(|c| c.gltf.is_none() && c.fill != enrich::ColorFill::Solid)
            .map(|c| {
                (
                    c.fill,
                    [c.from, c.to].map(|p| model.transform_point3(Vec3::from(p))),
                )
            }),
        _ => None,
    };
    let chosen_end = gradient
        .zip(edit.as_ref())
        .map(|((_, ends), e)| Affine3A::from_translation(ends[e.color_end.min(1)]));
    let chosen = edit
        .as_ref()
        .and_then(|e| e.locator)
        .filter(|&i| worlds.get(i).copied().flatten().is_some());
    let chosen_shape = edit
        .as_ref()
        .and_then(|e| e.shape)
        .filter(|&i| shape_worlds.get(i).copied().flatten().is_some());
    // The gizmo acts on the chosen shape while the Shapes tab shows; see
    // docs/features/shapes.md#3d-view.
    let target = if colors_active {
        chosen_end.filter(|_| tools.gizmo == Some(gizmo::Mode::Move))
    } else if paths_active {
        chosen_point.filter(|_| tools.gizmo == Some(gizmo::Mode::Move))
    } else if shapes_active {
        chosen_shape
            .filter(|&i| {
                edit.as_ref()
                    .is_some_and(|e| e.edit.shapes[i].shape != ShapeType::Hull)
            })
            .and_then(|i| shape_worlds[i])
    } else {
        chosen.and_then(|i| worlds[i])
    };
    let handles = match (target, tools.gizmo) {
        (Some(world), Some(mode)) if !locked && !painting && !drawing => {
            gizmo::Gizmo::new(&camera, world, mode)
        }
        _ => None,
    };
    if handles.is_none() {
        tools.drag = None;
    }

    // A press on a handle drags it instead of orbiting the camera; so does painting.
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let hovered = match (&tools.drag, &handles, pointer) {
        (Some(drag), _, _) => Some(drag.handle),
        (None, Some(handles), Some(p)) if rect.contains(p) => handles.hit(p),
        _ => None,
    };
    let mut pressed = false;
    if tools.drag.is_none()
        && ui.input(|i| i.pointer.primary_pressed())
        && let (Some(handle), Some(handles), Some(p)) = (hovered, &handles, pointer)
    {
        tools.drag = Some(gizmo::Drag::start(handle, handles, p));
        pressed = true;
    }
    let grabbing = tools.drag.is_some();
    let response = view_3d(ui, rect, preview, !grabbing && !painting && !drawing);
    if let Some(drag) = &mut tools.drag {
        let (down, delta) = ui.input(|i| (i.pointer.primary_down(), i.pointer.delta()));
        // The press's frame moves nothing: its delta led the pointer to the handle.
        let delta = if pressed { egui::Vec2::ZERO } else { delta };
        if !down {
            tools.drag = None;
        } else if let (Some(edit), Some(frames), Some(p)) = (&mut edit, &frames, pointer) {
            if colors_active {
                let mut sets = color_sets.clone();
                if let Some(set) = edit.color.and_then(|i| sets.get_mut(i))
                    && let Some(parent) = frames.parent(None)
                {
                    let point = if edit.color_end == 0 {
                        &mut set.from
                    } else {
                        &mut set.to
                    };
                    let mut moved = amod::Locator {
                        translation: *point,
                        ..amod::Locator::new(String::new())
                    };
                    drag.apply(&mut moved, parent, &camera, p, delta);
                    *point = moved.translation;
                    edit.set_color_sets(&sets, &model_colors);
                }
            } else if paths_active {
                if let (Some(k), Some(i)) = (edit.path, edit.point)
                    && let Some(path) = edit.edit.paths.get_mut(k)
                    && let Some(parent) = frames.parent(path.joint.as_deref())
                    && let Some(point) = path.points.get_mut(i)
                {
                    let mut moved = amod::Locator {
                        translation: *point,
                        ..amod::Locator::new(String::new())
                    };
                    drag.apply(&mut moved, parent, &camera, p, delta);
                    *point = moved.translation;
                }
            } else if shapes_active {
                if let Some(shape) = chosen_shape.map(|i| &mut edit.edit.shapes[i])
                    && let Some(parent) = frames.parent(shape.joint.as_deref())
                {
                    // Through a locator, which has the same position and rotation.
                    let mut moved = amod::Locator {
                        translation: shape.translation,
                        rotation: shape.rotation,
                        ..amod::Locator::new(String::new())
                    };
                    drag.apply(&mut moved, parent, &camera, p, delta);
                    (shape.translation, shape.rotation) = (moved.translation, moved.rotation);
                }
            } else if let Some(locator) = chosen.map(|i| &mut edit.edit.locators[i])
                && let Some(parent) = frames.parent(locator.joint.as_deref())
            {
                drag.apply(locator, parent, &camera, p, delta);
            }
            ui.ctx().request_repaint();
        }
    }

    // Painting: a ray each frame while the left button is down; see
    // docs/features/regions.md#painting.
    if painting
        && !tools.paint_awaiting
        && let Some(p) = pointer.filter(|p| rect.contains(*p))
        && ui.input(|i| i.pointer.primary_down())
        && response.is_pointer_button_down_on()
    {
        let (origin, dir) = camera.ray(p);
        preview.surface_ray = Some(preview3d::SurfaceRay {
            origin,
            dir,
            brush: tools.brush * camera.world_per_point(Vec3::ZERO),
        });
        tools.paint_awaiting = true;
        tools.erase = tools.rubber != ui.input(|i| i.modifiers.shift);
        ui.ctx().request_repaint();
    }

    // Drawing: a point where the press hits, then one every DRAW_STEP along the stroke; see
    // docs/features/paths.md#drawing.
    const DRAW_STEP: f32 = 24.0;
    let down = ui.input(|i| i.pointer.primary_down());
    if !down {
        tools.draw_last = None;
    }
    if drawing
        && !tools.draw_awaiting
        && down
        && response.is_pointer_button_down_on()
        && let Some(p) = pointer.filter(|p| rect.contains(*p))
        && tools
            .draw_last
            .is_none_or(|last| (p - last).length() >= DRAW_STEP)
    {
        let (origin, dir) = camera.ray(p);
        preview.surface_ray = Some(preview3d::SurfaceRay {
            origin,
            dir,
            brush: 0.0,
        });
        tools.draw_awaiting = true;
        tools.draw_last = Some(p);
        ui.ctx().request_repaint();
    }

    // Markers, nearest within reach picked by a click; see docs/features/locators.md#3d-view.
    let markers: Vec<_> = worlds
        .iter()
        .enumerate()
        .filter_map(|(i, world)| Some((i, gizmo::marker(&camera, (*world)?)?)))
        .collect();
    // Visible paths' points on screen: path, point, position.
    let path_dots: Vec<(usize, usize, egui::Pos2)> = match &edit {
        Some(edit) => edit
            .edit
            .paths
            .iter()
            .enumerate()
            .filter_map(|(p, path)| Some((p, path, path_parents.get(p).copied().flatten()?)))
            .flat_map(|(p, path, parent)| {
                let camera = &camera;
                path.points.iter().enumerate().filter_map(move |(i, q)| {
                    Some((
                        p,
                        i,
                        camera.project(parent.transform_point3(Vec3::from(*q)))?,
                    ))
                })
            })
            .collect(),
        None => Vec::new(),
    };
    // The chosen gradient's ends on screen.
    let end_dots: Vec<Option<egui::Pos2>> = gradient
        .map(|(_, ends)| ends.iter().map(|e| camera.project(*e)).collect())
        .unwrap_or_default();
    let picked_end = response
        .interact_pointer_pos()
        .filter(|_| response.clicked() && !grabbing)
        .and_then(|at| {
            end_dots
                .iter()
                .enumerate()
                .filter_map(|(k, dot)| Some((k, (*dot)? - at)))
                .map(|(k, d)| (k, d.length()))
                .filter(|(_, d)| *d <= gizmo::PICK)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(k, _)| k)
        });
    if let (Some(k), Some(edit)) = (picked_end, &mut edit) {
        edit.color_end = k;
    } else if response.clicked()
        && !grabbing
        && !painting
        && !drawing
        && let Some(edit) = &mut edit
        && frames.is_some()
        && let Some(at) = response.interact_pointer_pos()
    {
        let picked = markers
            .iter()
            .map(|(i, (dot, _))| (*i, (*dot - at).length()))
            .filter(|(_, d)| *d <= gizmo::PICK)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        let picked_point = path_dots
            .iter()
            .map(|(p, i, dot)| (*p, *i, (*dot - at).length()))
            .filter(|(.., d)| *d <= gizmo::PICK)
            .min_by(|a, b| a.2.total_cmp(&b.2));
        if picked.is_some() {
            edit.locator = picked;
            out.show_tab = Some(InspectorTab::Locators);
        } else if let Some((p, i, _)) = picked_point {
            if edit.path != Some(p) {
                tools.draw = false;
            }
            (edit.path, edit.point) = (Some(p), Some(i));
            out.show_tab = Some(InspectorTab::Paths);
        } else if tools.armed {
            let (origin, dir) = camera.ray(at);
            preview.surface_ray = Some(preview3d::SurfaceRay {
                origin,
                dir,
                brush: 0.0,
            });
            tools.awaiting = true;
            ui.ctx().request_repaint();
        } else {
            edit.locator = None;
        }
    }
    if ui.rect_contains_pointer(rect) && ui.memory(|m| m.focused().is_none()) {
        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                tools.gizmo = None;
                tools.armed = false;
                tools.paint = false;
                tools.draw = false;
            } else if locked {
            } else if i.key_pressed(egui::Key::W) {
                tools.toggle_gizmo(gizmo::Mode::Move);
            } else if i.key_pressed(egui::Key::E) && !paths_active && !colors_active {
                tools.toggle_gizmo(gizmo::Mode::Rotate);
            } else if i.key_pressed(egui::Key::S) {
                tools.armed = true;
            }
        });
    }

    // Models shown at visible locators; see docs/features/locators.md#attachment-preview.
    if let Some(edit) = &mut edit {
        let MeshEdit { edit, held, .. } = &mut **edit;
        for l in &edit.locators {
            if let Some(path) = &l.preview
                && show_locators
                && !hidden.contains(&l.name)
            {
                preview.attachments.push(preview3d::Attachment {
                    path: path.into(),
                    joint: l.joint.clone(),
                    transform: Transform::from_matrix(Mat4::from(gizmo::local(l))),
                    held: l
                        .preview_locator
                        .as_ref()
                        .and_then(|at| held.find(path, at)),
                });
            }
        }
    }

    // Regions: ask for their triangles, draw last frame's; see docs/features/regions.md#3d-view.
    preview.region_wants.clear();
    preview.region_view = false;
    preview.region_outline = None;
    if let (Some(edit), Some(geometry)) = (&edit, geometry) {
        preview.region_view = show_regions && !edit.edit.regions.is_empty();
        for (r, region) in edit.edit.regions.iter().enumerate() {
            if !show_regions || hidden_regions.contains(&region.name) {
                continue;
            }
            if edit.region == Some(r) {
                preview.region_outline = Some(r);
            }
            let color = hex_color32(&region.color).unwrap_or(TEXT);
            for (mesh, primitive, material, _) in &geometry.primitives {
                let whole = material
                    .as_ref()
                    .is_some_and(|m| region.materials.contains(m))
                    || region.submeshes.contains(&(*mesh, *primitive));
                let painted: Vec<u32> = region
                    .triangles
                    .range((*mesh, *primitive, 0)..=(*mesh, *primitive, u32::MAX))
                    .map(|(_, _, t)| *t)
                    .collect();
                if whole || !painted.is_empty() {
                    preview.region_wants.push(preview3d::RegionWant {
                        region: r,
                        color: [color.r(), color.g(), color.b()],
                        mesh: *mesh,
                        primitive: *primitive,
                        triangles: (!whole).then_some(painted),
                    });
                }
            }
        }
    }
    // The chosen color set on the model; see docs/features/vertex-colors.md#3d-view.
    preview.color_view = None;
    if colors_active
        && let (Some(edit), Some(geometry)) = (&edit, geometry)
        && let Some(i) = edit.color
    {
        let sets = edit.color_sets(&model_colors);
        if i < sets.len() {
            let key: ColorKey = (sets, i);
            let fresh = tools
                .color_view
                .as_ref()
                .is_some_and(|(path, shown, _)| *path == inspected.path && *shown == key);
            if !fresh {
                tools.color_generation += 1;
                let view = preview3d::ColorView {
                    generation: tools.color_generation,
                    corners: geometry.corner_colors(&key.0, i),
                };
                tools.color_view = Some((inspected.path.clone(), key, std::sync::Arc::new(view)));
            }
            preview.color_view = tools.color_view.as_ref().map(|(_, _, view)| view.clone());
        }
    }
    let painter = ui.painter_at(rect);
    if let Some(edit) = &edit {
        // The chosen region's outline; the colors are drawn by the 3D view.
        let eye = camera.position();
        let mut outlines = Vec::new();
        for (r, corners) in &preview.region_triangles {
            let Some(region) = edit.edit.regions.get(*r) else {
                continue;
            };
            let [a, b, c] = *corners;
            if (b - a).cross(c - a).dot(eye - a) <= 0.0 {
                continue;
            }
            let Some(points) = corners
                .iter()
                .map(|p| camera.project(*p))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let color = hex_color32(&region.color).unwrap_or(TEXT);
            outlines.extend((0..3).map(|k| ([points[k], points[(k + 1) % 3]], color)));
        }
        for (line, color) in outlines {
            painter.line_segment(line, Stroke::new(1.0, color));
        }
    }

    // Shapes as wireframes; see docs/features/shapes.md#3d-view.
    if let Some(edit) = &edit {
        for (i, shape) in edit.edit.shapes.iter().enumerate() {
            let Some(world) = shape_worlds.get(i).copied().flatten() else {
                continue;
            };
            let segments = match shape.shape {
                ShapeType::Hull => {
                    let hull = hulls.entry(shape.joint.clone()).or_insert_with(|| {
                        geometry
                            .and_then(|g| g.hull_points(shape.joint.as_deref()))
                            .and_then(|p| crate::hull::hull(&p))
                    });
                    hull.as_ref().map_or_else(Vec::new, |h| {
                        h.edges()
                            .into_iter()
                            .map(|[a, b]| [h.points[a], h.points[b]])
                            .collect()
                    })
                }
                other => shape_segments(other),
            };
            let color = locator_color32_of(shape.color.as_deref()).unwrap_or(TEXT_WEAK);
            let stroke = if chosen_shape == Some(i) {
                Stroke::new(2.0, color.lerp_to_gamma(egui::Color32::WHITE, 0.5))
            } else {
                Stroke::new(1.0, color)
            };
            for [a, b] in segments {
                if let (Some(a), Some(b)) = (
                    camera.project(world.transform_point3(a)),
                    camera.project(world.transform_point3(b)),
                ) {
                    painter.line_segment([a, b], stroke);
                }
            }
        }
    }

    // Paths: curves and points; see docs/features/paths.md#3d-view.
    if let Some(edit) = &edit {
        for (p, path) in edit.edit.paths.iter().enumerate() {
            let Some(parent) = path_parents.get(p).copied().flatten() else {
                continue;
            };
            let color = locator_color32_of(path.color.as_deref()).unwrap_or(TEXT_WEAK);
            let chosen_path = edit.path == Some(p);
            let (width, color) = if chosen_path {
                (2.0, color.lerp_to_gamma(egui::Color32::WHITE, 0.5))
            } else {
                (1.0, color)
            };
            if let Some(curve) =
                crate::path::Curve::new(&path.points, path.closed, path.tension, path.alpha)
            {
                let line: Vec<Option<egui::Pos2>> = curve
                    .polyline(16)
                    .into_iter()
                    .map(|q| camera.project(parent.transform_point3(Vec3::from(q))))
                    .collect();
                for pair in line.windows(2) {
                    if let [Some(a), Some(b)] = pair {
                        painter.line_segment([*a, *b], Stroke::new(width, color));
                    }
                }
            }
        }
        for (p, i, dot) in &path_dots {
            let path = &edit.edit.paths[*p];
            let color = locator_color32_of(path.color.as_deref()).unwrap_or(TEXT_WEAK);
            if edit.path == Some(*p) && edit.point == Some(*i) {
                painter.circle_filled(*dot, 7.0, color.lerp_to_gamma(egui::Color32::WHITE, 0.5));
                painter.circle_stroke(*dot, 7.0, Stroke::new(2.0, ACCENT));
            } else if edit.path == Some(*p) {
                painter.circle_filled(*dot, 5.0, color.lerp_to_gamma(egui::Color32::WHITE, 0.5));
            } else {
                painter.circle_filled(*dot, 5.0, color);
            }
        }
    }

    // The chosen gradient: a line between its ends, the radius around a radial one's center.
    if let Some((fill, ends)) = gradient
        && let [Some(from), Some(to)] = end_dots[..]
    {
        let stroke = Stroke::new(1.0, TEXT);
        painter.line_segment([from, to], stroke);
        if fill == enrich::ColorFill::Radial {
            let radius = (ends[1] - ends[0]).length() / camera.world_per_point(ends[0]);
            painter.circle_stroke(from, radius, stroke);
        }
    }
    if let Some(edit) = &edit {
        for (k, dot) in end_dots.iter().enumerate() {
            let Some(dot) = dot else { continue };
            if edit.color_end == k {
                painter.circle_filled(*dot, 7.0, TEXT);
                painter.circle_stroke(*dot, 7.0, Stroke::new(2.0, ACCENT));
            } else {
                painter.circle_filled(*dot, 5.0, TEXT);
            }
        }
    }

    // The brush at the pointer; see docs/features/regions.md#painting.
    if painting
        && let Some(edit) = &edit
        && let Some(region) = edit.region.and_then(|r| edit.edit.regions.get(r))
        && let Some(p) = pointer.filter(|p| rect.contains(*p))
    {
        let color = if tools.rubber != ui.input(|i| i.modifiers.shift) {
            TEXT
        } else {
            hex_color32(&region.color).unwrap_or(TEXT)
        };
        painter.circle_stroke(p, tools.brush, Stroke::new(1.0, color));
    }

    if let Some(edit) = &edit {
        for (i, (dot, ends)) in &markers {
            let locator = &edit.edit.locators[*i];
            let color = locator_color32(locator).unwrap_or(TEXT);
            paint_marker(
                &painter,
                *dot,
                ends,
                &locator.name,
                chosen == Some(*i),
                color,
            );
        }
    }
    if let (Some(handles), Some(mode)) = (&handles, tools.gizmo) {
        let style = |handle: gizmo::Handle, color: egui::Color32| {
            if hovered == Some(handle) {
                (3.0, color.lerp_to_gamma(egui::Color32::WHITE, 0.5))
            } else {
                (2.0, color)
            }
        };
        for (i, color) in MARKER_AXES.into_iter().enumerate() {
            match mode {
                gizmo::Mode::Move => {
                    if let Some(tip) = handles.arrows[i] {
                        let (width, color) = style(gizmo::Handle::Arrow(i), color);
                        let along = (tip - handles.center).normalized();
                        let side = along.rot90() * 4.0;
                        let base = tip - along * 6.0;
                        painter.line_segment([handles.center, base], Stroke::new(width, color));
                        painter.add(egui::Shape::convex_polygon(
                            vec![tip, base + side, base - side],
                            color,
                            Stroke::NONE,
                        ));
                    }
                }
                gizmo::Mode::Rotate => {
                    let (width, color) = style(gizmo::Handle::Ring(i), color);
                    for run in &handles.rings[i] {
                        painter.add(egui::Shape::line(run.clone(), Stroke::new(width, color)));
                    }
                }
            }
        }
        if mode == gizmo::Mode::Move {
            let (_, fill) = style(gizmo::Handle::Center, ACCENT);
            painter.circle_filled(handles.center, 7.0, fill);
        }
    }

    // The grid is always there to show.
    let counts = edit.as_ref().map_or([0, 0, 0, 0, 1], |e| {
        [
            e.edit.locators.len(),
            e.edit.shapes.len(),
            e.edit.regions.len(),
            e.edit.paths.len(),
            1,
        ]
    });
    let show: Vec<Tool> = [
        (Icon::MapPin, "Locators"),
        (Icon::Shapes, "Shapes"),
        (Icon::PaintBucket, "Regions"),
        (Icon::Spline, "Paths"),
        (Icon::Grid3x3, "Grid"),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (icon, name))| Tool {
        glyph: Glyph::Icon(icon),
        name: if counts[i] > 0 {
            name.into()
        } else {
            format!("{name} (none in this model)")
        },
        on: counts[i] > 0 && tools.show[i],
        enabled: counts[i] > 0,
    })
    .collect();
    let seen = vision_tools(*vision);
    match toolbar(ui, rect, &[&show, &seen]) {
        Some((0, button)) => {
            tools.show[button] = !tools.show[button];
            if !tools.show[2] {
                tools.paint = false;
            }
            if !tools.show[3] {
                tools.draw = false;
            }
            ui.ctx().request_repaint();
        }
        Some((_, button)) => {
            toggle_vision(vision, button);
            ui.ctx().request_repaint();
        }
        None => {}
    }
    if let ModelStatus::Failed(error) = &preview.model_status {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            format!("Could not load the model: {error}"),
            FontId::proportional(14.0),
            TEXT_WEAK,
        );
    }
    if painting || !preview.region_wants.is_empty() {
        ui.ctx().request_repaint();
    }
    out.save = edit.and_then(|edit| mesh_commit(ui, edit));
    out
}

/// The lines of a box, sphere or capsule in its own space; see docs/features/shapes.md#3d-view.
fn shape_segments(shape: ShapeType) -> Vec<[bevy::math::Vec3; 2]> {
    use bevy::math::Vec3;
    use std::f32::consts::{PI, TAU};
    let mut out = Vec::new();
    /// `segments` pieces of the arc from `from` to `to` radians, in the plane of `u` and `v`.
    fn arc(out: &mut Vec<[Vec3; 2]>, at: Vec3, u: Vec3, v: Vec3, to: f32, segments: usize) {
        let point = |k: usize| {
            let t = to * k as f32 / segments as f32;
            at + u * t.cos() + v * t.sin()
        };
        for k in 0..segments {
            out.push([point(k), point(k + 1)]);
        }
    }
    match shape {
        ShapeType::Box { size } => {
            let h = Vec3::from(size) / 2.0;
            let corner = |i: usize| {
                Vec3::new(
                    if i & 1 == 0 { -h.x } else { h.x },
                    if i & 2 == 0 { -h.y } else { h.y },
                    if i & 4 == 0 { -h.z } else { h.z },
                )
            };
            for i in 0..8 {
                for bit in [1, 2, 4] {
                    if i & bit == 0 {
                        out.push([corner(i), corner(i | bit)]);
                    }
                }
            }
        }
        ShapeType::Sphere { radius } => {
            let r = radius;
            arc(&mut out, Vec3::ZERO, Vec3::Y * r, Vec3::Z * r, TAU, 48);
            arc(&mut out, Vec3::ZERO, Vec3::Z * r, Vec3::X * r, TAU, 48);
            arc(&mut out, Vec3::ZERO, Vec3::X * r, Vec3::Y * r, TAU, 48);
        }
        ShapeType::Capsule { radius: r, height } => {
            let top = Vec3::Y * height / 2.0;
            for end in [top, -top] {
                arc(&mut out, end, Vec3::X * r, Vec3::Z * r, TAU, 48);
            }
            for side in [Vec3::X * r, -Vec3::X * r, Vec3::Z * r, -Vec3::Z * r] {
                out.push([top + side, -top + side]);
            }
            for u in [Vec3::X * r, Vec3::Z * r] {
                arc(&mut out, top, u, Vec3::Y * r, PI, 24);
                arc(&mut out, -top, u, -Vec3::Y * r, PI, 24);
            }
        }
        ShapeType::Hull => {}
    }
    out
}

/// Colors of a locator's X, Y and Z axes.
const MARKER_AXES: [egui::Color32; 3] = [CHANNEL_R, CHANNEL_G, CHANNEL_B];

/// Marker colors offered for locators; see docs/features/locators.md#locators-tab.
const LOCATOR_COLORS: [&str; 8] = [
    "#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4", "#3b82f6", "#a855f7", "#ec4899",
];

fn hex_color32(text: &str) -> Option<egui::Color32> {
    amod::locator_color(text).map(|[r, g, b]| egui::Color32::from_rgb(r, g, b))
}

/// The color of a `#rrggbb` setting, when set.
fn locator_color32_of(color: Option<&str>) -> Option<egui::Color32> {
    hex_color32(color?)
}

/// A locator's marker color, when it has one.
fn locator_color32(locator: &amod::Locator) -> Option<egui::Color32> {
    hex_color32(locator.color.as_deref()?)
}

/// A locator's dot, axis lines and name; see docs/features/locators.md#3d-view.
fn paint_marker(
    painter: &egui::Painter,
    dot: egui::Pos2,
    ends: &[Option<egui::Pos2>; 3],
    name: &str,
    chosen: bool,
    color: egui::Color32,
) {
    for (end, axis) in ends.iter().zip(MARKER_AXES) {
        if let Some(end) = end {
            painter.line_segment([dot, *end], Stroke::new(1.5, axis));
        }
    }
    if chosen {
        painter.circle_filled(dot, 7.0, color);
        painter.circle_stroke(dot, 7.0, Stroke::new(2.0, ACCENT));
    } else {
        painter.circle_filled(dot, 5.0, color);
    }
    let galley = painter.layout_no_wrap(name.to_owned(), FontId::proportional(12.0), TEXT);
    let at = pos2(dot.x + 12.0, dot.y - galley.size().y / 2.0);
    painter.rect_filled(
        Rect::from_min_size(at, galley.size()).expand2(vec2(4.0, 1.0)),
        3,
        BG_BAR.gamma_multiply(0.7),
    );
    painter.galley(at, galley, TEXT);
}

/// Adds a locator at the model's origin and chooses it; see docs/features/locators.md#locators-tab.
fn add_locator(edit: &mut MeshEdit) {
    let locators = &mut edit.edit.locators;
    locators.push(amod::Locator::new(amod::free_locator_name(locators)));
    edit.locator = Some(locators.len() - 1);
}

/// Makes or updates the left / right counterpart of locator `i` and chooses it; see
/// docs/features/locators.md#mirroring.
fn mirror_locator(
    edit: &mut MeshEdit,
    i: usize,
    joints: &[String],
    frames: &ModelFrames,
    axis: usize,
) {
    let source = edit.edit.locators[i].clone();
    let Some(name) = gizmo::mirror_name(&source.name) else {
        return;
    };
    let joint = source
        .joint
        .as_ref()
        .map(|j| gizmo::mirror_joint(j, joints).unwrap_or_else(|| j.clone()));
    let (Some(from), Some(to)) = (
        frames.parent(source.joint.as_deref()),
        frames.parent(joint.as_deref()),
    ) else {
        return;
    };
    let world = gizmo::mirrored(frames.model, from * gizmo::local(&source), axis);
    let mut mirrored = amod::Locator {
        name,
        joint,
        ..source.clone()
    };
    gizmo::set_local(&mut mirrored, to.inverse() * world);
    mirrored.scale = source.scale;
    let locators = &mut edit.edit.locators;
    let k = match locators.iter().position(|l| l.name == mirrored.name) {
        Some(k) => {
            locators[k] = mirrored;
            k
        }
        None => {
            locators.push(mirrored);
            locators.len() - 1
        }
    };
    edit.locator = Some(k);
}

/// What a locator card's header asks for.
#[derive(Clone, Copy)]
enum CardEvent {
    Fold(usize),
    Gizmo(usize, gizmo::Mode),
    Delete(usize),
    Revert(usize),
}

/// The Locators tab; returns the sidecar to write. See docs/features/locators.md#locators-tab.
fn locators_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    frames: Option<&ModelFrames>,
    tools: &mut LocatorTools,
    animating: bool,
) -> Option<Amod> {
    let joints = &inspected.inspection.joints;
    let models = &inspected.inspection.models;
    let hidden = tools.hidden.entry(inspected.path.clone()).or_default();
    let edit = match &mut inspected.mesh {
        Some(Ok(edit)) => edit,
        Some(Err(e)) => {
            placeholder(ui, e);
            return None;
        }
        None => {
            placeholder(ui, "No locators");
            return None;
        }
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                ui.horizontal(|ui| {
                    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add");
                    if ui.add(add).clicked() {
                        add_locator(edit);
                        tools.show[0] = true;
                    }
                    let mut place = egui::Button::image_and_text(
                        Icon::Crosshair.image(14.0, TEXT),
                        "Place on surface",
                    );
                    if tools.armed {
                        place = place.fill(ACCENT.gamma_multiply(0.25));
                    }
                    let place = ui
                        .add_enabled(!animating, place)
                        .on_hover_text("Click the model to add a locator there (S)");
                    if place.clicked() {
                        tools.armed = !tools.armed;
                    }
                });
                ui.add_space(8.0);
                if edit.edit.locators.is_empty() {
                    ui.weak("No locators");
                }
                let mut event = None;
                for i in 0..edit.edit.locators.len() {
                    let open = edit.locator == Some(i);
                    let name = edit.edit.locators[i].name.clone();
                    let dot = locator_color32(&edit.edit.locators[i]).unwrap_or(TEXT);
                    let from = edit
                        .inherited(i)
                        .map(|(f, changed)| (f.source.label().to_owned(), changed));
                    let mut visible = !hidden.contains(&name);
                    let gizmo = tools.gizmo.filter(|_| open);
                    let movable = !animating && visible;
                    let mut gizmo_click = None;
                    let (_, folded) = ui
                        .push_id(("locator", i), |ui| {
                            card(
                                ui,
                                Some(open),
                                RichText::new(&name),
                                |ui| {
                                    let (icon, tint) = if visible {
                                        (Icon::Eye, TEXT)
                                    } else {
                                        (Icon::EyeOff, TEXT_WEAK)
                                    };
                                    let (rect, eye) =
                                        ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                                    icon.image(14.0, tint).paint_at(ui, rect.shrink(1.0));
                                    if eye.on_hover_text("Visible").clicked() {
                                        visible = !visible;
                                    }
                                    let (rect, _) =
                                        ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                    ui.painter().circle_filled(rect.center(), 5.0, dot);
                                },
                                |ui, open| {
                                    if open {
                                        match &from {
                                            Some((label, true)) => {
                                                let tip = format!("Revert to {label}");
                                                if icon_button(ui, Icon::RefreshCw, &tip, true)
                                                    .clicked()
                                                {
                                                    event = Some(CardEvent::Revert(i));
                                                }
                                            }
                                            Some((label, false)) => {
                                                let tip = format!("From {label}; remove it there");
                                                icon_button(ui, Icon::Trash2, &tip, false);
                                            }
                                            None => {
                                                if icon_button(ui, Icon::Trash2, "Delete", true)
                                                    .clicked()
                                                {
                                                    event = Some(CardEvent::Delete(i));
                                                }
                                            }
                                        }
                                    }
                                },
                                |ui| {
                                    let fields = LocatorFields {
                                        joints,
                                        models,
                                        frames,
                                        animating,
                                        gizmo,
                                        movable,
                                    };
                                    gizmo_click = locator_fields(
                                        ui,
                                        edit,
                                        i,
                                        &fields,
                                        &mut tools.mirror_axis,
                                    );
                                },
                            )
                        })
                        .inner;
                    if folded {
                        event = Some(CardEvent::Fold(i));
                    }
                    if let Some(mode) = gizmo_click {
                        event = Some(CardEvent::Gizmo(i, mode));
                    }
                    if visible == hidden.contains(&name) {
                        if visible {
                            hidden.remove(&name);
                        } else {
                            hidden.insert(name);
                        }
                    }
                }
                match event {
                    Some(CardEvent::Fold(i)) => {
                        edit.locator = (edit.locator != Some(i)).then_some(i);
                    }
                    Some(CardEvent::Gizmo(i, mode)) => {
                        if edit.locator == Some(i) && tools.gizmo == Some(mode) {
                            tools.gizmo = None;
                        } else {
                            edit.locator = Some(i);
                            tools.gizmo = Some(mode);
                        }
                    }
                    Some(CardEvent::Delete(i)) => {
                        edit.edit.locators.remove(i);
                        edit.locator = None;
                    }
                    Some(CardEvent::Revert(i)) => {
                        if let Some((from, _)) = edit.inherited(i) {
                            edit.edit.locators[i] = from.locator.clone();
                        }
                    }
                    None => {}
                }
                ui.weak("Click a locator in the view to choose it.");
            });
        });
    mesh_commit(ui, edit)
}

/// What the locator fields choose from.
struct LocatorFields<'a> {
    joints: &'a [String],
    models: &'a [String],
    frames: Option<&'a ModelFrames>,
    /// An animation plays: transforms are locked.
    animating: bool,
    /// The gizmo on this locator, when on.
    gizmo: Option<gizmo::Mode>,
    /// Its gizmo can be turned on: not animating, and visible.
    movable: bool,
}

/// The body of locator `i`'s card: where it comes from, gizmo, name, color, joint, transform,
/// preview and mirroring. Returns the gizmo button clicked.
fn locator_fields(
    ui: &mut Ui,
    edit: &mut MeshEdit,
    i: usize,
    fields: &LocatorFields,
    mirror_axis: &mut usize,
) -> Option<gizmo::Mode> {
    use bevy::math::{EulerRot, Quat};
    let LocatorFields {
        joints,
        models,
        frames,
        animating,
        gizmo,
        movable,
    } = *fields;
    let saved_name = edit.saved.locators.get(i).map(|l| l.name.clone());
    // An inherited locator is found by its name.
    let inherited = edit
        .inherited(i)
        .map(|(f, changed)| (f.source.clone(), changed));
    if let Some((source, changed)) = &inherited {
        let from = match source {
            presets::Source::File => "From the file".to_owned(),
            source => format!("From {}", source.label()),
        };
        let changed = if *changed { ", changed here" } else { "" };
        ui.weak(format!("{from}{changed}"));
    }
    let mut clicked = None;
    prop_row(ui, "Gizmo", |ui, _| {
        let modes = [
            (Icon::Move3d, "Move (W)", gizmo::Mode::Move),
            (Icon::Rotate3d, "Rotate (E)", gizmo::Mode::Rotate),
        ];
        let buttons = modes.map(|(icon, tip, mode)| (icon, tip, gizmo == Some(mode)));
        clicked = button_group(ui, &buttons, movable).map(|b| modes[b].2);
    });
    let locators = &mut edit.edit.locators;
    let mut renamed = false;
    prop_row(ui, "Name", |ui, width| {
        let field = egui::TextEdit::singleline(&mut locators[i].name);
        renamed = ui
            .add_enabled_ui(inherited.is_none(), |ui| {
                ui.add_sized(vec2(width, 22.0), field)
            })
            .inner
            .lost_focus();
    });
    // An empty or taken name gets the old one back.
    let name = &locators[i].name;
    if renamed && (name.is_empty() || locators.iter().filter(|l| l.name == *name).count() > 1) {
        locators[i].name = saved_name
            .filter(|old| locators.iter().all(|l| l.name != *old))
            .unwrap_or_else(|| amod::free_locator_name(locators));
    }

    let locator = &mut locators[i];
    // Edited as text; split into tags when it loses focus.
    prop_row(ui, "Tags", |ui, width| {
        let id = ui.id().with("locator_tags");
        let mut text = ui
            .data(|d| d.get_temp::<String>(id))
            .unwrap_or_else(|| locator.tags.join(", "));
        let field = egui::TextEdit::singleline(&mut text).hint_text("weapon, left");
        let response = ui.add_sized(vec2(width, 22.0), field);
        if response.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text));
        } else {
            if response.lost_focus() {
                locator.tags = amod::parse_tags(&text);
            }
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
    color_row(ui, &mut locator.color, true);
    ui.add_enabled_ui(!animating, |ui| {
        let mut joint = locator.joint.clone();
        prop_row(ui, "Joint", |ui, width| {
            egui::ComboBox::from_id_salt("locator_joint")
                .width(width - 8.0)
                .selected_text(joint.as_deref().unwrap_or("None (model space)"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut joint, None, "None (model space)");
                    for name in joints {
                        ui.selectable_value(&mut joint, Some(name.clone()), name);
                    }
                });
        });
        if joint != locator.joint {
            // Onto the new parent; see docs/features/locators.md#locators-tab.
            locator.joint = joint;
            locator.translation = [0.0; 3];
            locator.rotation = [0.0, 0.0, 0.0, 1.0];
        }

        vec3_row(ui, "Position", &mut locator.translation, 0.001, 3, None);
        let rotation = Quat::from_array(locator.rotation);
        let rotation = if rotation.length_squared() > 0.0 {
            rotation.normalize()
        } else {
            Quat::IDENTITY
        };
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);
        // Rounding noise would show as −0.0.
        let mut degrees = [x, y, z].map(|a| {
            let d = a.to_degrees();
            if d.abs() < 1e-3 { 0.0 } else { d }
        });
        let before = degrees;
        vec3_row(ui, "Rotation", &mut degrees, 0.5, 1, None);
        if degrees != before {
            let [x, y, z] = degrees.map(f32::to_radians);
            locator.rotation = Quat::from_euler(EulerRot::XYZ, x, y, z).to_array();
        }
        vec3_row(ui, "Scale", &mut locator.scale, 0.01, 3, Some(0.001));
    });

    prop_row(ui, "Preview", |ui, width| {
        let mut preview = locator.preview.clone();
        egui::ComboBox::from_id_salt("locator_preview")
            .width(width - 8.0)
            .selected_text(preview.as_deref().unwrap_or("None"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut preview, None, "None");
                for model in models {
                    ui.selectable_value(&mut preview, Some(model.clone()), model);
                }
            });
        if preview != locator.preview {
            locator.preview = preview;
            locator.preview_locator = None;
        }
    });
    prop_row(ui, "Attach by", |ui, width| {
        let names: Result<Vec<String>, String> = match &locator.preview {
            Some(path) => match edit.held.get(path) {
                Ok(held) => Ok(held.iter().map(|l| l.name.clone()).collect()),
                Err(e) => Err(e.clone()),
            },
            None => Ok(Vec::new()),
        };
        let missing = locator
            .preview_locator
            .clone()
            .filter(|name| !names.as_ref().is_ok_and(|n| n.contains(name)));
        let mut at = locator.preview_locator.clone();
        let selected = match (&at, &missing) {
            (None, _) => "Origin".to_owned(),
            (Some(name), Some(_)) => format!("{name} (missing)"),
            (Some(name), None) => name.clone(),
        };
        ui.add_enabled_ui(locator.preview.is_some(), |ui| {
            let response = egui::ComboBox::from_id_salt("locator_attach")
                .width(width - 8.0)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut at, None, "Origin");
                    if let Some(name) = &missing {
                        ui.selectable_value(
                            &mut at,
                            Some(name.clone()),
                            format!("{name} (missing)"),
                        );
                    }
                    for name in names.iter().flatten() {
                        ui.selectable_value(&mut at, Some(name.clone()), name);
                    }
                })
                .response;
            if let Err(e) = &names {
                response.on_hover_text(e);
            }
        });
        if at != locator.preview_locator {
            locator.preview_locator = at;
        }
    });

    let mirrorable = !animating && frames.is_some() && gizmo::mirror_name(&locator.name).is_some();
    let mut mirror = false;
    prop_row(ui, "Mirror", |ui, _| {
        mirror = icon_button(ui, Icon::FlipHorizontal2, "Mirror", mirrorable).clicked();
        ui.add_enabled_ui(!animating, |ui| {
            egui::ComboBox::from_id_salt("mirror_axis")
                .width(48.0)
                .selected_text(["X", "Y", "Z"][*mirror_axis])
                .show_ui(ui, |ui| {
                    for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                        ui.selectable_value(mirror_axis, axis, label);
                    }
                });
        });
    });
    if animating {
        ui.add_space(4.0);
        ui.weak("Transforms are locked while an animation plays. Choose Rest pose to edit them.");
    }
    if mirror && let Some(frames) = frames {
        mirror_locator(edit, i, joints, frames, *mirror_axis);
    }
    clicked
}

/// Icon buttons joined in a row like the canvas toolbar's groups, 32 × 24 px each, `ACCENT`
/// while on; returns the clicked one.
fn button_group(ui: &mut Ui, buttons: &[(Icon, &str, bool)], enabled: bool) -> Option<usize> {
    const BUTTON: Vec2 = vec2(32.0, 24.0);
    const RADIUS: u8 = 6;
    let (bar, _) = ui.allocate_exact_size(
        vec2(BUTTON.x * buttons.len() as f32, BUTTON.y),
        Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(bar, RADIUS, BG_BAR);
    let last = buttons.len() - 1;
    let mut clicked = None;
    for (i, (icon, tip, on)) in buttons.iter().enumerate() {
        let rect = Rect::from_min_size(bar.min + vec2(BUTTON.x * i as f32, 0.0), BUTTON);
        let sense = if enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let response = ui
            .interact(rect, ui.id().with(("button_group", i)), sense)
            .on_hover_text(*tip);
        let outer = |edge: bool| if edge { RADIUS } else { 0 };
        let radius = egui::CornerRadius {
            nw: outer(i == 0),
            sw: outer(i == 0),
            ne: outer(i == last),
            se: outer(i == last),
        };
        let tint = match (enabled, *on, response.hovered()) {
            (false, _, _) => TEXT_WEAK.gamma_multiply(0.5),
            (true, true, _) => {
                ui.painter().rect_filled(rect, radius, ACCENT);
                BG_BAR
            }
            (true, false, true) => {
                ui.painter().rect_filled(rect, radius, BG_HEADER);
                TEXT
            }
            (true, false, false) => TEXT_WEAK,
        };
        if i > 0 {
            ui.painter()
                .vline(rect.left(), rect.y_range(), Stroke::new(1.0, STROKE));
        }
        icon.image(16.0, tint)
            .paint_at(ui, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)));
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// The marker color swatches: none, the offered colors, and a color set by hand.
fn color_row(ui: &mut Ui, color: &mut Option<String>, none: bool) {
    prop_row(ui, "Color", |ui, width| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let mut options: Vec<Option<String>> = none
            .then_some(None)
            .into_iter()
            .chain(LOCATOR_COLORS.map(|c| Some(c.to_owned())))
            .collect();
        if let Some(own) = color
            .as_ref()
            .filter(|c| !LOCATOR_COLORS.contains(&c.as_str()))
        {
            options.push(Some(own.clone()));
        }
        let gaps = 3.0 * (options.len() - 1) as f32;
        let size = ((width - gaps) / options.len() as f32).clamp(8.0, 18.0);
        for option in options {
            let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::click());
            let painter = ui.painter();
            match option.as_deref().and_then(hex_color32) {
                Some(fill) => {
                    painter.rect_filled(rect, 4, fill);
                }
                None => {
                    let rect = rect.shrink(0.5);
                    painter.rect_stroke(rect, 4, Stroke::new(1.0, TEXT_WEAK), StrokeKind::Inside);
                    painter.line_segment(
                        [rect.left_bottom(), rect.right_top()],
                        Stroke::new(1.0, TEXT_WEAK),
                    );
                }
            }
            if *color == option {
                painter.rect_stroke(rect, 4, Stroke::new(2.0, TEXT), StrokeKind::Inside);
            }
            let tip = option.clone().unwrap_or_else(|| "None".into());
            if response.on_hover_text(tip).clicked() {
                *color = option;
            }
        }
    });
}

/// The eye toggle and color dot that start an enrichment card's header.
fn eye_and_dot(ui: &mut Ui, visible: &mut bool, dot: egui::Color32) {
    let (icon, tint) = if *visible {
        (Icon::Eye, TEXT)
    } else {
        (Icon::EyeOff, TEXT_WEAK)
    };
    let (rect, eye) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
    icon.image(14.0, tint).paint_at(ui, rect.shrink(1.0));
    if eye.on_hover_text("Visible").clicked() {
        *visible = !*visible;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, dot);
}

/// Shows `name` as visible or hidden in `hidden`, as `visible` says.
fn set_visible(hidden: &mut HashSet<String>, name: String, visible: bool) {
    if visible {
        hidden.remove(&name);
    } else {
        hidden.insert(name);
    }
}

/// A name field; when it loses focus empty or equal to one of `others`, it gets `saved` back
/// (or a free name from `base`).
fn name_row(ui: &mut Ui, name: &mut String, saved: Option<&str>, others: &[&str], base: &str) {
    let mut done = false;
    prop_row(ui, "Name", |ui, width| {
        done = ui
            .add_sized(vec2(width, 22.0), egui::TextEdit::singleline(name))
            .lost_focus();
    });
    if done && (name.is_empty() || others.contains(&name.as_str())) {
        *name = saved
            .filter(|old| !others.contains(old))
            .map(str::to_owned)
            .unwrap_or_else(|| enrich::free_name(base, |n| others.contains(&n)));
    }
}

/// A text field of comma-separated tags, split when it loses focus.
fn tags_row(ui: &mut Ui, tags: &mut Vec<String>) {
    prop_row(ui, "Tags", |ui, width| {
        let id = ui.id().with("tags");
        let mut text = ui
            .data(|d| d.get_temp::<String>(id))
            .unwrap_or_else(|| tags.join(", "));
        let field = egui::TextEdit::singleline(&mut text).hint_text("metal, breakable");
        let response = ui.add_sized(vec2(width, 22.0), field);
        if response.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text));
        } else {
            if response.lost_focus() {
                *tags = amod::parse_tags(&text);
            }
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
}

/// A rotation as Euler angles in degrees (X, then Y, then Z).
fn rotation_row(ui: &mut Ui, rotation: &mut [f32; 4]) {
    use bevy::math::{EulerRot, Quat};
    let q = Quat::from_array(*rotation);
    let q = if q.length_squared() > 0.0 {
        q.normalize()
    } else {
        Quat::IDENTITY
    };
    let (x, y, z) = q.to_euler(EulerRot::XYZ);
    // Rounding noise would show as −0.0.
    let mut degrees = [x, y, z].map(|a| {
        let d = a.to_degrees();
        if d.abs() < 1e-3 { 0.0 } else { d }
    });
    let before = degrees;
    vec3_row(ui, "Rotation", &mut degrees, 0.5, 1, None);
    if degrees != before {
        let [x, y, z] = degrees.map(f32::to_radians);
        *rotation = Quat::from_euler(EulerRot::XYZ, x, y, z).to_array();
    }
}

/// One positive length.
fn length_row(ui: &mut Ui, label: &str, value: &mut f32) {
    prop_row(ui, label, |ui, width| {
        let field = egui::DragValue::new(value)
            .speed(0.01)
            .fixed_decimals(3)
            .range(0.001..=f32::MAX);
        ui.add_sized(vec2(width.min(120.0), 22.0), field);
    });
}

/// Edits `properties`; see docs/features/properties.md#properties-tab.
fn properties_editor(ui: &mut Ui, properties: &mut Properties) {
    let keys: Vec<String> = properties.keys().cloned().collect();
    let (mut remove, mut rename) = (None, None);
    for (i, key) in keys.iter().enumerate() {
        let Some(value) = properties.get_mut(key) else {
            continue;
        };
        ui.push_id(("property", i), |ui| {
            ui.allocate_ui_with_layout(
                vec2(ui.available_width(), 26.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let width = ui.available_width();
                    let key_width = ((width - 64.0 - 24.0 - 12.0) * 0.5).max(40.0);
                    // The key, applied when its field loses focus.
                    let id = ui.id().with("key");
                    let mut text = ui
                        .data(|d| d.get_temp::<String>(id))
                        .unwrap_or_else(|| key.clone());
                    let field =
                        ui.add_sized(vec2(key_width, 22.0), egui::TextEdit::singleline(&mut text));
                    if field.has_focus() {
                        ui.data_mut(|d| d.insert_temp(id, text));
                    } else {
                        if field.lost_focus() {
                            let new = text.trim().to_owned();
                            if !new.is_empty() && new != *key && !keys.contains(&new) {
                                rename = Some((key.clone(), new));
                            }
                        }
                        ui.data_mut(|d| d.remove::<String>(id));
                    }
                    let mut kind = value.type_index();
                    egui::ComboBox::from_id_salt("type")
                        .width(56.0)
                        .selected_text(PropValue::TYPES[kind])
                        .show_ui(ui, |ui| {
                            for (k, name) in PropValue::TYPES.into_iter().enumerate() {
                                ui.selectable_value(&mut kind, k, name);
                            }
                        });
                    if kind != value.type_index() {
                        *value = value.converted(kind);
                    }
                    let rest = (ui.available_width() - 28.0).max(30.0);
                    match value {
                        PropValue::Bool(b) => {
                            ui.add_sized(vec2(rest, 22.0), egui::Checkbox::without_text(b));
                        }
                        PropValue::Int(n) => {
                            ui.add_sized(vec2(rest, 22.0), egui::DragValue::new(n));
                        }
                        PropValue::Float(f) => {
                            let field = egui::DragValue::new(f).speed(0.01).fixed_decimals(3);
                            ui.add_sized(vec2(rest, 22.0), field);
                        }
                        PropValue::Text(t) => {
                            ui.add_sized(vec2(rest, 22.0), egui::TextEdit::singleline(t));
                        }
                    }
                    if icon_button(ui, Icon::Trash2, "Delete", true).clicked() {
                        remove = Some(key.clone());
                    }
                },
            );
        });
    }
    if let Some(key) = remove {
        properties.remove(&key);
    }
    if let Some((old, new)) = rename
        && let Some(value) = properties.remove(&old)
    {
        properties.insert(new, value);
    }
    if properties.is_empty() {
        ui.weak("No properties");
        ui.add_space(4.0);
    }
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add property");
    if ui.add(add).clicked() {
        let key = enrich::free_name("property", |k| properties.contains_key(k));
        properties.insert(key, PropValue::Float(0.0));
    }
}

/// The Properties tab; see docs/features/properties.md#properties-tab.
fn properties_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Amod> {
    let Some(Ok(edit)) = &mut inspected.mesh else {
        placeholder(ui, "No properties");
        return None;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                properties_editor(ui, &mut edit.edit.properties);
            });
        });
    mesh_commit(ui, edit)
}

/// The Shapes tab; see docs/features/shapes.md#shapes-tab.
fn shapes_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    tools: &mut LocatorTools,
    animating: bool,
) -> Option<Amod> {
    let joints = &inspected.inspection.joints;
    let geometry = inspected.geometry.as_ref();
    let extent = geometry.map_or(1.0, |g| g.extent);
    let hulls = &mut inspected.hulls;
    let hidden = tools
        .hidden_shapes
        .entry(inspected.path.clone())
        .or_default();
    let Some(Ok(edit)) = &mut inspected.mesh else {
        placeholder(ui, "No shapes");
        return None;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add shape");
                if ui.add(add).clicked() {
                    let shapes = &mut edit.edit.shapes;
                    let name = enrich::free_name("shape", |n| shapes.iter().any(|s| s.name == n));
                    shapes.push(enrich::Shape::new(name, extent));
                    edit.shape = Some(shapes.len() - 1);
                    tools.show[1] = true;
                }
                ui.add_space(8.0);
                if edit.edit.shapes.is_empty() {
                    ui.weak("No shapes");
                }
                let mut event = None;
                for i in 0..edit.edit.shapes.len() {
                    let open = edit.shape == Some(i);
                    let name = edit.edit.shapes[i].name.clone();
                    let dot = locator_color32_of(edit.edit.shapes[i].color.as_deref())
                        .unwrap_or(TEXT_WEAK);
                    let mut visible = !hidden.contains(&name);
                    let gizmo = tools.gizmo.filter(|_| open);
                    let movable = !animating && visible;
                    let mut gizmo_click = None;
                    let mut delete = false;
                    let (_, folded) = ui
                        .push_id(("shape", i), |ui| {
                            card(
                                ui,
                                Some(open),
                                RichText::new(&name),
                                |ui| eye_and_dot(ui, &mut visible, dot),
                                |ui, open| {
                                    delete = open
                                        && icon_button(ui, Icon::Trash2, "Delete", true).clicked();
                                },
                                |ui| {
                                    gizmo_click = shape_fields(
                                        ui, edit, i, joints, extent, hulls, geometry, gizmo,
                                        movable, animating,
                                    );
                                },
                            )
                        })
                        .inner;
                    if folded {
                        event = Some(CardEvent::Fold(i));
                    }
                    if let Some(mode) = gizmo_click {
                        event = Some(CardEvent::Gizmo(i, mode));
                    }
                    if delete {
                        event = Some(CardEvent::Delete(i));
                    }
                    if visible == hidden.contains(&name) {
                        set_visible(hidden, name, visible);
                    }
                }
                match event {
                    Some(CardEvent::Fold(i)) => edit.shape = (edit.shape != Some(i)).then_some(i),
                    Some(CardEvent::Gizmo(i, mode)) => {
                        if edit.shape == Some(i) && tools.gizmo == Some(mode) {
                            tools.gizmo = None;
                        } else {
                            edit.shape = Some(i);
                            tools.gizmo = Some(mode);
                        }
                    }
                    Some(CardEvent::Delete(i)) => {
                        edit.edit.shapes.remove(i);
                        edit.shape = None;
                    }
                    Some(CardEvent::Revert(_)) | None => {}
                }
            });
        });
    mesh_commit(ui, edit)
}

/// The body of shape `i`'s card; returns the gizmo button clicked.
#[allow(clippy::too_many_arguments)]
fn shape_fields(
    ui: &mut Ui,
    edit: &mut MeshEdit,
    i: usize,
    joints: &[String],
    extent: f32,
    hulls: &mut HashMap<Option<String>, Option<crate::hull::Hull>>,
    geometry: Option<&model::ModelGeometry>,
    gizmo: Option<gizmo::Mode>,
    movable: bool,
    animating: bool,
) -> Option<gizmo::Mode> {
    let saved = edit.saved.shapes.get(i).map(|s| s.name.clone());
    let others: Vec<String> = edit
        .edit
        .shapes
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != i)
        .map(|(_, s)| s.name.clone())
        .collect();
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    let shape = &mut edit.edit.shapes[i];
    let mut clicked = None;
    if shape.shape != ShapeType::Hull {
        prop_row(ui, "Gizmo", |ui, _| {
            let modes = [
                (Icon::Move3d, "Move (W)", gizmo::Mode::Move),
                (Icon::Rotate3d, "Rotate (E)", gizmo::Mode::Rotate),
            ];
            let buttons = modes.map(|(icon, tip, mode)| (icon, tip, gizmo == Some(mode)));
            clicked = button_group(ui, &buttons, movable).map(|b| modes[b].2);
        });
    }
    name_row(ui, &mut shape.name, saved.as_deref(), &others, "shape");
    prop_row(ui, "Kind", |ui, width| {
        let field = egui::TextEdit::singleline(&mut shape.kind).hint_text("hitbox");
        ui.add_sized(vec2(width, 22.0), field);
    });
    ui.add_enabled_ui(!animating, |ui| {
        prop_row(ui, "Type", |ui, width| {
            let mut index = shape.shape.index();
            egui::ComboBox::from_id_salt("shape_type")
                .width(width - 8.0)
                .selected_text(ShapeType::NAMES[index])
                .show_ui(ui, |ui| {
                    for (k, name) in ShapeType::NAMES.into_iter().enumerate() {
                        ui.selectable_value(&mut index, k, name);
                    }
                });
            if index != shape.shape.index() {
                shape.shape = shape.shape.with_index(index, extent);
            }
        });
    });
    color_row(ui, &mut shape.color, true);
    ui.add_enabled_ui(!animating, |ui| {
        let mut joint = shape.joint.clone();
        prop_row(ui, "Joint", |ui, width| {
            egui::ComboBox::from_id_salt("shape_joint")
                .width(width - 8.0)
                .selected_text(joint.as_deref().unwrap_or("None (model space)"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut joint, None, "None (model space)");
                    for name in joints {
                        ui.selectable_value(&mut joint, Some(name.clone()), name);
                    }
                });
        });
        if joint != shape.joint {
            // Onto the new parent, as locators.
            shape.joint = joint;
            shape.translation = [0.0; 3];
            shape.rotation = [0.0, 0.0, 0.0, 1.0];
        }
        match &mut shape.shape {
            ShapeType::Hull => {}
            other => {
                vec3_row(ui, "Position", &mut shape.translation, 0.001, 3, None);
                rotation_row(ui, &mut shape.rotation);
                match other {
                    ShapeType::Box { size } => vec3_row(ui, "Size", size, 0.01, 3, Some(0.001)),
                    ShapeType::Sphere { radius } => length_row(ui, "Radius", radius),
                    ShapeType::Capsule { radius, height } => {
                        length_row(ui, "Radius", radius);
                        length_row(ui, "Height", height);
                    }
                    ShapeType::Hull => {}
                }
            }
        }
    });
    if shape.shape == ShapeType::Hull {
        let hull = hulls.entry(shape.joint.clone()).or_insert_with(|| {
            geometry
                .and_then(|g| g.hull_points(shape.joint.as_deref()))
                .and_then(|p| crate::hull::hull(&p))
        });
        let around = shape
            .joint
            .as_deref()
            .map_or("the model".to_owned(), str::to_owned);
        ui.weak(match hull {
            Some(hull) => format!("{} points around {around}", hull.points.len()),
            None => format!("No hull around {around}"),
        });
    }
    if animating {
        ui.weak("Transforms are locked while an animation plays. Choose Rest pose to edit them.");
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Properties").size(13.0).color(TEXT_WEAK));
    properties_editor(ui, &mut shape.properties);
    clicked
}

/// The Paths tab; see docs/features/paths.md#paths-tab.
fn paths_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    tools: &mut LocatorTools,
    animating: bool,
) -> Option<Amod> {
    let joints = &inspected.inspection.joints;
    let extent = inspected.geometry.as_ref().map_or(1.0, |g| g.extent);
    let LocatorTools {
        hidden_paths,
        gizmo,
        show,
        draw,
        armed,
        ..
    } = tools;
    let hidden = hidden_paths.entry(inspected.path.clone()).or_default();
    let Some(Ok(edit)) = &mut inspected.mesh else {
        placeholder(ui, "No paths");
        return None;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add path");
                if ui.add(add).clicked() {
                    let paths = &mut edit.edit.paths;
                    let name = enrich::free_name("path", |n| paths.iter().any(|p| p.name == n));
                    paths.push(enrich::Path::new(name, extent));
                    (edit.path, edit.point) = (Some(paths.len() - 1), None);
                    *draw = false;
                    show[3] = true;
                }
                ui.add_space(8.0);
                if edit.edit.paths.is_empty() {
                    ui.weak("No paths");
                }
                let mut event = None;
                for i in 0..edit.edit.paths.len() {
                    let open = edit.path == Some(i);
                    let name = edit.edit.paths[i].name.clone();
                    let dot = locator_color32_of(edit.edit.paths[i].color.as_deref())
                        .unwrap_or(TEXT_WEAK);
                    let mut visible = !hidden.contains(&name);
                    let mode = gizmo.filter(|_| open);
                    let movable = !animating && visible;
                    let mut gizmo_click = None;
                    let mut delete = false;
                    let (_, folded) = ui
                        .push_id(("path", i), |ui| {
                            card(
                                ui,
                                Some(open),
                                RichText::new(&name),
                                |ui| eye_and_dot(ui, &mut visible, dot),
                                |ui, open| {
                                    delete = open
                                        && icon_button(ui, Icon::Trash2, "Delete", true).clicked();
                                },
                                |ui| {
                                    gizmo_click = path_fields(
                                        ui, edit, i, joints, mode, movable, animating, draw, armed,
                                        show,
                                    );
                                },
                            )
                        })
                        .inner;
                    if folded {
                        event = Some(CardEvent::Fold(i));
                    }
                    if let Some(mode) = gizmo_click {
                        event = Some(CardEvent::Gizmo(i, mode));
                    }
                    if delete {
                        event = Some(CardEvent::Delete(i));
                    }
                    if visible == hidden.contains(&name) {
                        set_visible(hidden, name, visible);
                    }
                }
                match event {
                    Some(CardEvent::Fold(i)) => {
                        edit.path = (edit.path != Some(i)).then_some(i);
                        edit.point = None;
                        *draw = false;
                    }
                    Some(CardEvent::Gizmo(i, mode)) => {
                        if edit.path == Some(i) && *gizmo == Some(mode) {
                            *gizmo = None;
                        } else {
                            if edit.path != Some(i) {
                                (edit.path, edit.point) = (Some(i), None);
                                *draw = false;
                            }
                            *gizmo = Some(mode);
                        }
                    }
                    Some(CardEvent::Delete(i)) => {
                        edit.edit.paths.remove(i);
                        (edit.path, edit.point) = (None, None);
                        *draw = false;
                    }
                    Some(CardEvent::Revert(_)) | None => {}
                }
            });
        });
    mesh_commit(ui, edit)
}

/// The body of path `i`'s card; returns the gizmo button clicked.
#[allow(clippy::too_many_arguments)]
fn path_fields(
    ui: &mut Ui,
    edit: &mut MeshEdit,
    i: usize,
    joints: &[String],
    gizmo: Option<gizmo::Mode>,
    movable: bool,
    animating: bool,
    draw: &mut bool,
    armed: &mut bool,
    show: &mut [bool; 5],
) -> Option<gizmo::Mode> {
    let saved = edit.saved.paths.get(i).map(|p| p.name.clone());
    let others: Vec<String> = edit
        .edit
        .paths
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != i)
        .map(|(_, p)| p.name.clone())
        .collect();
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    let chosen = &mut edit.point;
    let path = &mut edit.edit.paths[i];
    let mut clicked = None;
    prop_row(ui, "Gizmo", |ui, _| {
        let buttons = [(Icon::Move3d, "Move (W)", gizmo == Some(gizmo::Mode::Move))];
        clicked =
            button_group(ui, &buttons, movable && chosen.is_some()).map(|_| gizmo::Mode::Move);
    });
    name_row(ui, &mut path.name, saved.as_deref(), &others, "path");
    prop_row(ui, "Kind", |ui, width| {
        let field = egui::TextEdit::singleline(&mut path.kind).hint_text("rope");
        ui.add_sized(vec2(width, 22.0), field);
    });
    color_row(ui, &mut path.color, true);
    ui.add_enabled_ui(!animating, |ui| {
        let mut joint = path.joint.clone();
        prop_row(ui, "Joint", |ui, width| {
            egui::ComboBox::from_id_salt("path_joint")
                .width(width - 8.0)
                .selected_text(joint.as_deref().unwrap_or("None (model space)"))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut joint, None, "None (model space)");
                    for name in joints {
                        ui.selectable_value(&mut joint, Some(name.clone()), name);
                    }
                });
        });
        path.joint = joint;
    });
    prop_row(ui, "Closed", |ui, _| {
        ui.checkbox(&mut path.closed, "");
    });
    // Sliders sized to the row, so they never widen the panel.
    prop_row(ui, "Tension", |ui, width| {
        ui.spacing_mut().slider_width = (width - 64.0).max(40.0);
        ui.add(egui::Slider::new(&mut path.tension, 0.0..=1.0).fixed_decimals(2));
    });
    prop_row(ui, "Alpha", |ui, width| {
        ui.spacing_mut().slider_width = (width - 64.0).max(40.0);
        ui.add(egui::Slider::new(&mut path.alpha, 0.0..=1.0).fixed_decimals(2))
            .on_hover_text("0 uniform, 0.5 centripetal, 1 chordal");
    });
    match crate::path::Curve::new(&path.points, path.closed, path.tension, path.alpha) {
        Some(curve) => ui.weak(format!(
            "{} {}, length {:.3}",
            curve.segment_count(),
            if curve.segment_count() == 1 {
                "segment"
            } else {
                "segments"
            },
            curve.length()
        )),
        None => ui.weak(format!(
            "No curve: needs at least {} points",
            crate::path::min_points(path.closed)
        )),
    };
    ui.add_space(6.0);
    ui.label(RichText::new("Points").size(13.0).color(TEXT_WEAK));
    let mut remove = None;
    ui.add_enabled_ui(!animating, |ui| {
        let removable = path.points.len() > 2;
        for (k, point) in path.points.iter_mut().enumerate() {
            ui.push_id(("point", k), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let color = if *chosen == Some(k) {
                        ACCENT
                    } else {
                        TEXT_WEAK
                    };
                    let number = egui::Label::new(RichText::new(format!("{}", k + 1)).color(color))
                        .sense(Sense::click());
                    let response = ui.add_sized(vec2(22.0, 22.0), number);
                    if response.clicked() {
                        *chosen = Some(k);
                    }
                    // Three gaps and the 24 px remove button, with room to spare.
                    let each = ((ui.available_width() - 40.0) / 3.0).max(20.0);
                    for value in point.iter_mut() {
                        let field = egui::DragValue::new(value).speed(0.001).fixed_decimals(3);
                        if ui.add_sized(vec2(each, 22.0), field).gained_focus() {
                            *chosen = Some(k);
                        }
                    }
                    if icon_button(ui, Icon::X, "Remove point", removable).clicked() {
                        remove = Some(k);
                    }
                });
            });
        }
    });
    if let Some(k) = remove {
        path.points.remove(k);
        *chosen = chosen
            .filter(|c| *c != k)
            .map(|c| if c > k { c - 1 } else { c });
    }
    ui.add_enabled_ui(!animating, |ui| {
        ui.horizontal(|ui| {
            let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add point");
            if ui.add(add).clicked() {
                let at = chosen
                    .map_or(path.points.len(), |c| c + 1)
                    .min(path.points.len());
                path.points
                    .insert(at, new_point(&path.points, at, path.closed));
                *chosen = Some(at);
                show[3] = true;
            }
            let mut button = egui::Button::image_and_text(Icon::PenLine.image(14.0, TEXT), "Draw");
            if *draw {
                button = button.fill(ACCENT.gamma_multiply(0.25));
            }
            let response = ui
                .add(button)
                .on_hover_text("Click the model to add points there");
            if response.clicked() {
                *draw = !*draw;
                *armed = false;
                show[3] = true;
            }
        });
    });
    if animating {
        ui.weak("Points are locked while an animation plays. Choose Rest pose to edit them.");
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Properties").size(13.0).color(TEXT_WEAK));
    properties_editor(ui, &mut path.properties);
    clicked
}

/// A new point to insert at `at`: halfway to the next point, or past the end by the last
/// segment's offset; see docs/features/paths.md#paths-tab.
fn new_point(points: &[[f32; 3]], at: usize, closed: bool) -> [f32; 3] {
    use bevy::math::Vec3;
    let get = |i: usize| points.get(i).copied().map(Vec3::from);
    let point = match (at.checked_sub(1).and_then(get), get(at)) {
        (Some(before), Some(after)) => (before + after) / 2.0,
        (Some(last), None) if closed && !points.is_empty() => (last + Vec3::from(points[0])) / 2.0,
        (Some(last), None) => {
            let previous = at.checked_sub(2).and_then(get);
            last + previous.map_or(Vec3::X * 0.1, |p| last - p)
        }
        (None, Some(first)) => first,
        (None, None) => Vec3::ZERO,
    };
    point.to_array()
}

/// Height of the animation timeline; see docs/features/animation-events.md#timeline.
const TIMELINE_HEIGHT: f32 = 132.0;

/// The animation timeline's state.
#[derive(Default)]
struct Timeline {
    /// The model and animation shown; another one starts afresh.
    shown: Option<(PathBuf, usize)>,
    paused: bool,
    /// The event being dragged, by index in the sidecar's.
    drag: Option<usize>,
    /// The playhead last frame, and when each event (by index) was last passed.
    last_time: Option<f32>,
    flashes: HashMap<usize, Instant>,
}

/// The previous (or next) multiple of `step` strictly before (after) `t`; a time within a
/// hundredth of a step of a multiple counts as on it.
fn step_time(t: f32, step: f32, forward: bool) -> f32 {
    let slack = step / 100.0;
    let k = if forward {
        ((t + slack) / step).floor() + 1.0
    } else {
        ((t - slack) / step).ceil() - 1.0
    };
    k * step
}

/// `0.5 s`, `1 s`, `0.25 s`: seconds without trailing zeros.
fn seconds_label(t: f32) -> String {
    let text = format!("{t:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text} s")
}

/// The timeline under a model's 3D view: playback and the chosen animation's events; returns
/// the sidecar to write. See docs/features/animation-events.md#timeline.
fn timeline(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    preview: &mut Preview3d,
    state: &mut Timeline,
    animation: Option<usize>,
) -> Option<Amod> {
    const MARKER_REACH: f32 = 6.0;
    const FLASH: f32 = 0.25;
    ui.painter().rect_filled(rect, 0, BG_PANEL);
    ui.painter()
        .hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, STROKE));
    let inner = rect.shrink2(vec2(8.0, 0.0));
    let mut ui = ui.new_child(
        UiBuilder::new()
            .id_salt("timeline")
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    let ui = &mut ui;
    ui.spacing_mut().item_spacing.y = 2.0;
    let Some((index, (name, duration))) = animation.and_then(|i| {
        let (name, duration) = inspected.inspection.animations.get(i)?;
        Some((i, (name.clone(), duration.max(f32::EPSILON))))
    }) else {
        state.shown = None;
        ui.allocate_ui_with_layout(
            vec2(inner.width(), 28.0),
            Layout::left_to_right(Align::Center),
            |ui| ui.weak("Choose an animation to edit its events"),
        );
        return None;
    };
    let Some(Ok(edit)) = &mut inspected.mesh else {
        return None;
    };
    // Another model or animation: play it from its start, nothing chosen.
    let key = (inspected.path.clone(), index);
    if state.shown.as_ref() != Some(&key) {
        *state = Timeline {
            shown: Some(key),
            ..Timeline::default()
        };
        edit.event = None;
    }
    let mut now = preview.animation_time.unwrap_or(0.0).min(duration);
    // Space plays and pauses.
    if ui.rect_contains_pointer(rect)
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Space))
    {
        state.paused = !state.paused;
    }
    // `,` and `.` step to the previous / next hundredth of a second (thousandth with Shift),
    // paused; `E` toggles an event at the playhead.
    let mut toggle = false;
    if !ui.ctx().egui_wants_keyboard_input() {
        // Shift as it was when the key went down.
        let (back, forward, fine) = ui.input(|i| {
            let mut out = (false, false, false);
            for event in &i.events {
                if let egui::Event::Key {
                    key: key @ (egui::Key::Comma | egui::Key::Period),
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    out.0 |= *key == egui::Key::Comma;
                    out.1 |= *key == egui::Key::Period;
                    out.2 |= modifiers.shift;
                }
            }
            out
        });
        if back || forward {
            let step = if fine { 0.001 } else { 0.01 };
            now = step_time(now, step, forward).clamp(0.0, duration);
            preview.animation_seek = Some(now);
            state.paused = true;
        }
        toggle = ui.input(|i| i.key_pressed(egui::Key::E) && i.modifiers.is_none());
    }

    // Header.
    let mut add = false;
    ui.allocate_ui_with_layout(
        vec2(inner.width(), 28.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (icon, tip) = if state.paused {
                (Icon::Play, "Play (Space)")
            } else {
                (Icon::Pause, "Pause (Space)")
            };
            if icon_button(ui, icon, tip, true).clicked() {
                state.paused = !state.paused;
            }
            ui.label(
                RichText::new(format!("{now:.3} / {duration:.3} s"))
                    .monospace()
                    .color(TEXT),
            );
            ui.label(RichText::new(&name).color(TEXT_WEAK));
            let button = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add event");
            add = ui.add(button).clicked();
        },
    );

    let x0 = inner.left() + MARKER_REACH;
    let x1 = inner.right() - MARKER_REACH;
    let x_of = |t: f32| x0 + t / duration * (x1 - x0);
    let time_of = |x: f32| {
        let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0) * duration;
        (t * 1000.0).round() / 1000.0
    };
    let pointer = ui.input(|i| i.pointer.latest_pos());

    // Ruler: press or drag moves the playhead and pauses.
    let (ruler, ruler_response) =
        ui.allocate_exact_size(vec2(inner.width(), 20.0), Sense::click_and_drag());
    if (ruler_response.is_pointer_button_down_on() || ruler_response.clicked())
        && let Some(p) = ruler_response.interact_pointer_pos().or(pointer)
    {
        now = time_of(p.x);
        preview.animation_seek = Some(now);
        state.paused = true;
    }
    let per_second = (x1 - x0) / duration;
    let step = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]
        .into_iter()
        .find(|s| s * per_second >= 60.0)
        .unwrap_or(30.0);
    let painter = ui.painter().clone();
    let mut k = 0;
    while k as f32 * step <= duration + 1e-4 {
        let t = k as f32 * step;
        let x = x_of(t);
        painter.vline(
            x,
            (ruler.bottom() - 6.0)..=ruler.bottom(),
            Stroke::new(1.0, TEXT_WEAK),
        );
        painter.text(
            pos2(x + 3.0, ruler.top() + 2.0),
            Align2::LEFT_TOP,
            seconds_label(t),
            FontId::proportional(11.0),
            TEXT_WEAK,
        );
        k += 1;
    }

    // Track: markers to choose, drag, add and delete.
    let (track, track_response) =
        ui.allocate_exact_size(vec2(inner.width(), 32.0), Sense::click_and_drag());
    painter.rect_filled(track, 4, BG_HEADER);
    let mine: Vec<usize> = (0..edit.edit.events.len())
        .filter(|&i| edit.edit.events[i].animation == name)
        .collect();
    let marker_at = |events: &[enrich::Event], x: f32| {
        mine.iter()
            .map(|&i| (i, (x_of(events[i].time) - x).abs()))
            .filter(|(_, d)| *d <= MARKER_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    if track_response.double_clicked()
        && let Some(p) = pointer
    {
        match marker_at(&edit.edit.events, p.x) {
            Some(i) => {
                edit.edit.events.remove(i);
                edit.event = None;
            }
            None => {
                edit.edit.events.push(enrich::Event {
                    animation: name.clone(),
                    name: "event".into(),
                    time: time_of(p.x),
                    properties: Properties::new(),
                });
                edit.event = Some(edit.edit.events.len() - 1);
            }
        }
        state.drag = None;
    } else if ui.input(|i| i.pointer.primary_pressed())
        && track_response.hovered()
        && let Some(p) = pointer
    {
        state.drag = marker_at(&edit.edit.events, p.x);
        if state.drag.is_some() {
            edit.event = state.drag;
        }
    }
    if let Some(i) = state.drag {
        match (ui.input(|i| i.pointer.primary_down()), pointer) {
            (true, Some(p)) if i < edit.edit.events.len() => {
                if track_response.dragged() {
                    edit.edit.events[i].time = time_of(p.x);
                }
            }
            _ => state.drag = None,
        }
    }
    if toggle {
        let at = edit
            .edit
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| e.animation == name)
            .map(|(i, e)| (i, (e.time - now).abs()))
            .filter(|(_, d)| *d <= 0.0005)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        match at {
            Some(i) => {
                edit.edit.events.remove(i);
                edit.event = None;
                state.flashes.clear();
            }
            None => add = true,
        }
    }
    if add {
        edit.edit.events.push(enrich::Event {
            animation: name.clone(),
            name: "event".into(),
            time: (now * 1000.0).round() / 1000.0,
            properties: Properties::new(),
        });
        edit.event = Some(edit.edit.events.len() - 1);
    }

    // Events the playhead passed since last frame flash.
    if let Some(last) = state.last_time
        && !state.paused
    {
        for (i, event) in edit.edit.events.iter().enumerate() {
            let t = event.time;
            let passed = if now >= last {
                last < t && t <= now
            } else {
                t > last || t <= now
            };
            if passed && event.animation == name {
                state.flashes.insert(i, Instant::now());
            }
        }
    }
    state.last_time = Some(now);
    state
        .flashes
        .retain(|_, at| at.elapsed().as_secs_f32() < FLASH);
    for (i, event) in edit.edit.events.iter().enumerate() {
        if event.animation != name {
            continue;
        }
        let center = pos2(x_of(event.time.min(duration)), track.center().y);
        let lit = edit.event == Some(i) || state.flashes.contains_key(&i);
        let color = if lit { ACCENT } else { TEXT };
        let r = 5.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                center + vec2(0.0, -r),
                center + vec2(r, 0.0),
                center + vec2(0.0, r),
                center + vec2(-r, 0.0),
            ],
            color,
            Stroke::NONE,
        ));
        painter.text(
            center + vec2(r + 3.0, 0.0),
            Align2::LEFT_CENTER,
            &event.name,
            FontId::proportional(11.0),
            TEXT_WEAK,
        );
    }
    if !state.flashes.is_empty() {
        ui.ctx().request_repaint();
    }

    // Playhead.
    let x = x_of(now);
    painter.vline(x, ruler.top()..=track.bottom(), Stroke::new(1.0, ACCENT));
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 4.0, ruler.top()),
            pos2(x + 4.0, ruler.top()),
            pos2(x, ruler.top() + 6.0),
        ],
        ACCENT,
        Stroke::NONE,
    ));

    // The chosen event's fields.
    ui.add_space(4.0);
    let chosen = edit
        .event
        .filter(|&i| edit.edit.events.get(i).is_some_and(|e| e.animation == name));
    ui.allocate_ui_with_layout(
        vec2(inner.width(), 24.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let Some(i) = chosen else {
                ui.weak("Double-click the track to add an event");
                return;
            };
            let mut delete = false;
            let event = &mut edit.edit.events[i];
            ui.label(RichText::new("Name").color(TEXT_WEAK));
            ui.add_sized(
                vec2(160.0, 22.0),
                egui::TextEdit::singleline(&mut event.name),
            );
            ui.label(RichText::new("Time").color(TEXT_WEAK));
            ui.add_sized(
                vec2(80.0, 22.0),
                egui::DragValue::new(&mut event.time)
                    .range(0.0..=duration)
                    .speed(0.01)
                    .fixed_decimals(3)
                    .suffix(" s"),
            );
            if icon_button(ui, Icon::Trash2, "Delete", true).clicked() {
                delete = true;
            }
            if delete {
                edit.edit.events.remove(i);
                edit.event = None;
            }
        },
    );
    mesh_commit(ui, edit)
}

/// The Vertex colors tab; see docs/features/vertex-colors.md#vertex-colors-tab.
fn vertex_colors_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    tools: &mut LocatorTools,
) -> Option<Amod> {
    use bevy::math::Vec3;
    use std::cell::Cell;
    let (model, bounds) = inspected
        .geometry
        .as_ref()
        .map_or((Vec::new(), [Vec3::ZERO; 2]), |g| {
            (g.color_sets.clone(), g.bounds)
        });
    let Some(Ok(edit)) = &mut inspected.mesh else {
        placeholder(ui, "No color sets");
        return None;
    };
    let gizmo = tools.gizmo;
    let mut sets = edit.color_sets(&model);
    let saved = vertex_colors::full_list(&edit.saved.colors, &model);
    edit.color_cards
        .resize_with(sets.len(), GradientCard::default);
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let add =
                    egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add color set");
                if ui.add(add).clicked() {
                    let name = enrich::free_name("colors", |n| sets.iter().any(|s| s.name == n));
                    let [min, max] = bounds;
                    let middle = (min + max) / 2.0;
                    let from = Vec3::new(middle.x, min.y, middle.z).to_array();
                    let to = Vec3::new(middle.x, max.y, middle.z).to_array();
                    sets.push(enrich::ColorSet::new(name, from, to));
                    edit.color_cards.push(GradientCard::default());
                    edit.color = Some(sets.len() - 1);
                    edit.color_end = 0;
                }
                ui.add_space(8.0);
                if sets.is_empty() {
                    ui.weak("No color sets");
                }
                let names: Vec<String> = sets.iter().map(|s| s.name.clone()).collect();
                let into: Vec<Option<String>> = vertex_colors::bases(&sets)
                    .into_iter()
                    .map(|n| n.map(str::to_owned))
                    .collect();
                let fold = Cell::new(None);
                let delete = Cell::new(None);
                let dragging = Cell::new(None);
                let dropped = Cell::new(None);
                let mut gizmo_click = false;
                let mut rects = Vec::with_capacity(sets.len());
                for i in 0..sets.len() {
                    let others: Vec<&str> = names
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, n)| n.as_str())
                        .collect();
                    // Bases it may blend with: not itself, nor sets blending with it.
                    let candidates: Vec<String> = (0..sets.len())
                        .filter(|&j| j != i && !vertex_colors::blends_with(&sets, j, i))
                        .map(|j| names[j].clone())
                        .collect();
                    let old = saved
                        .get(i)
                        .filter(|s| s.gltf == sets[i].gltf)
                        .map(|s| s.name.clone());
                    let gltf = sets[i].gltf;
                    let missing = gltf.is_some_and(|n| !model.contains(&n));
                    let link = match (&sets[i].blend, &into[i]) {
                        (Some(blend), _) => Some((TEXT, format!("Blends with {}", blend.with))),
                        (None, Some(top)) => Some((
                            TEXT_WEAK,
                            format!("Blended into {top}: not written on its own"),
                        )),
                        _ => None,
                    };
                    let mut mini = edit.color_cards[i].mini.take();
                    let shown = sets[i].clone();
                    let (rect, folded) = ui
                        .push_id(("colors", i), |ui| {
                            card(
                                ui,
                                Some(edit.color == Some(i)),
                                RichText::new(&names[i]),
                                |ui| {
                                    let grip = drag_handle(ui);
                                    if grip.dragged() {
                                        dragging.set(Some(i));
                                    }
                                    if grip.drag_stopped() {
                                        dropped.set(Some(i));
                                    }
                                    if let Some((color, hint)) = &link {
                                        let (rect, response) = ui
                                            .allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                                        Icon::Link2.image(14.0, *color).paint_at(ui, rect);
                                        response.on_hover_text(hint);
                                    }
                                },
                                |ui, open| {
                                    if open
                                        && (gltf.is_none() || missing)
                                        && icon_button(ui, Icon::Trash2, "Delete", true).clicked()
                                    {
                                        delete.set(Some(i));
                                    }
                                    if gltf.is_some() {
                                        ui.add_space(4.0);
                                        ui.label(RichText::new("glTF").size(11.0).color(TEXT_WEAK));
                                    } else if !open {
                                        ui.add_space(4.0);
                                        color_set_preview(ui, &shown, &mut mini);
                                    }
                                },
                                |ui| match gltf {
                                    Some(n) => gltf_set_fields(
                                        ui,
                                        &mut sets[i].name,
                                        n,
                                        old.as_deref(),
                                        &others,
                                        missing,
                                    ),
                                    None => {
                                        let card = &mut edit.color_cards[i];
                                        name_row(
                                            ui,
                                            &mut sets[i].name,
                                            old.as_deref(),
                                            &others,
                                            "colors",
                                        );
                                        gizmo_click |= color_set_fields(
                                            ui,
                                            &mut sets[i],
                                            card,
                                            &mut edit.color_end,
                                            gizmo,
                                            bounds,
                                            &candidates,
                                        );
                                    }
                                },
                            )
                        })
                        .inner;
                    edit.color_cards[i].mini = mini;
                    rects.push(rect);
                    if folded {
                        fold.set(Some(i));
                    }
                }
                // A renamed set keeps the blends using it.
                for (i, old) in names.iter().enumerate() {
                    let new = sets[i].name.clone();
                    if new != *old {
                        for set in &mut sets {
                            if let Some(blend) = &mut set.blend
                                && blend.with == *old
                            {
                                blend.with.clone_from(&new);
                            }
                        }
                    }
                }
                if let Some(i) = fold.get() {
                    edit.color = (edit.color != Some(i)).then_some(i);
                    edit.color_end = 0;
                }
                if gizmo_click {
                    tools.toggle_gizmo(gizmo::Mode::Move);
                }
                if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
                    let set = sets.remove(from);
                    sets.insert(to, set);
                    let card = edit.color_cards.remove(from);
                    edit.color_cards.insert(to, card);
                    edit.color = edit.color.map(|c| {
                        if c == from {
                            to
                        } else if from < c && c <= to {
                            c - 1
                        } else if to <= c && c < from {
                            c + 1
                        } else {
                            c
                        }
                    });
                }
                if let Some(i) = delete.get() {
                    let gone = sets.remove(i).name;
                    for set in &mut sets {
                        if set.blend.as_ref().is_some_and(|b| b.with == gone) {
                            set.blend = None;
                        }
                    }
                    edit.color_cards.remove(i);
                    edit.color = None;
                }
            });
        });
    edit.set_color_sets(&sets, &model);
    mesh_commit(ui, edit)
}

/// A glTF set's body: its name (empty gives `COLOR_n` back) and where it comes from.
fn gltf_set_fields(
    ui: &mut Ui,
    name: &mut String,
    n: u32,
    saved: Option<&str>,
    others: &[&str],
    missing: bool,
) {
    let attribute = format!("COLOR_{n}");
    let mut done = false;
    prop_row(ui, "Name", |ui, width| {
        let field = egui::TextEdit::singleline(name);
        done = ui.add_sized(vec2(width, 22.0), field).lost_focus();
    });
    if done {
        if name.is_empty() {
            name.clone_from(&attribute);
        }
        if others.contains(&name.as_str()) {
            *name = saved
                .filter(|old| !others.contains(old))
                .unwrap_or(&attribute)
                .to_owned();
        }
    }
    ui.weak(if missing {
        format!("{attribute} is not in the model")
    } else {
        format!("From the glTF's {attribute}")
    });
}

/// A color set's 48 × 12 preview: its gradient, first stop on the left, or its color.
fn color_set_preview(
    ui: &mut Ui,
    set: &enrich::ColorSet,
    mini: &mut Option<(Gradient, [usize; 2], egui::TextureHandle)>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(48.0, 12.0), Sense::hover());
    color_picker::checker(ui, rect, 3);
    if set.fill == enrich::ColorFill::Solid {
        let color = color_picker::to_color32(set.color);
        ui.painter().rect_filled(rect, 3, color);
    } else {
        let gradient = Gradient {
            name: String::new(),
            dither: Dither::None,
            stops: set.stops.clone(),
            modifiers: Vec::new(),
            ..Gradient::default()
        };
        let texture = gradient_texture(ui, mini, &gradient, rect.size());
        egui::Image::from_texture((texture.id(), rect.size()))
            .corner_radius(3)
            .paint_at(ui, rect);
    }
}

/// An authored color set's fields after its name; returns whether Move was clicked.
fn color_set_fields(
    ui: &mut Ui,
    set: &mut enrich::ColorSet,
    card: &mut GradientCard,
    end: &mut usize,
    gizmo: Option<gizmo::Mode>,
    bounds: [bevy::math::Vec3; 2],
    candidates: &[String],
) -> bool {
    use enrich::{Blend, BlendMode, ColorFill};
    prop_row(ui, "Fill", |ui, width| {
        egui::ComboBox::from_id_salt("color_fill")
            .width(width - 8.0)
            .selected_text(set.fill.name())
            .show_ui(ui, |ui| {
                for fill in ColorFill::ALL {
                    ui.selectable_value(&mut set.fill, fill, fill.name());
                }
            });
    });
    // See docs/features/vertex-colors.md#blending.
    let mut with = set.blend.as_ref().map(|b| b.with.clone());
    prop_row(ui, "Blend with", |ui, width| {
        egui::ComboBox::from_id_salt("color_blend")
            .width(width - 8.0)
            .selected_text(with.as_deref().unwrap_or("None"))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut with, None, "None");
                for name in candidates {
                    ui.selectable_value(&mut with, Some(name.clone()), name);
                }
            });
    });
    match (with, &mut set.blend) {
        (None, blend) => *blend = None,
        (Some(with), Some(blend)) => blend.with = with,
        (Some(with), blend) => {
            *blend = Some(Blend {
                with,
                mode: BlendMode::Mix,
                amount: 1.0,
            })
        }
    }
    if let Some(blend) = &mut set.blend {
        prop_row(ui, "Mode", |ui, width| {
            egui::ComboBox::from_id_salt("color_blend_mode")
                .width(width - 8.0)
                .selected_text(blend.mode.name())
                .show_ui(ui, |ui| {
                    for mode in BlendMode::ALL {
                        ui.selectable_value(&mut blend.mode, mode, mode.name());
                    }
                });
        });
        prop_row(ui, "Amount", |ui, width| {
            ui.spacing_mut().slider_width = (width - 64.0).max(40.0);
            ui.add(egui::Slider::new(&mut blend.amount, 0.0..=1.0).fixed_decimals(2));
        });
    }
    if set.fill == ColorFill::Solid {
        ui.add_space(4.0);
        card.picker.show(ui, &mut set.color);
        return false;
    }
    let mut clicked = false;
    prop_row(ui, "Gizmo", |ui, _| {
        let buttons = [(Icon::Move3d, "Move (W)", gizmo == Some(gizmo::Mode::Move))];
        clicked = button_group(ui, &buttons, true).is_some();
    });
    let labels = if set.fill == ColorFill::Radial {
        ["Center", "Edge"]
    } else {
        ["From", "To"]
    };
    for (k, label) in labels.into_iter().enumerate() {
        let point = if k == 0 { &mut set.from } else { &mut set.to };
        ui.push_id(("end", k), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let color = if *end == k { ACCENT } else { TEXT_WEAK };
                let name =
                    egui::Label::new(RichText::new(label).color(color)).sense(Sense::click());
                if ui.add_sized(vec2(44.0, 22.0), name).clicked() {
                    *end = k;
                }
                let each = ((ui.available_width() - 12.0) / 3.0).max(20.0);
                for value in point.iter_mut() {
                    let field = egui::DragValue::new(value).speed(0.001).fixed_decimals(3);
                    if ui.add_sized(vec2(each, 22.0), field).gained_focus() {
                        *end = k;
                    }
                }
            });
        });
    }
    prop_row(ui, "Fit", |ui, _| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
            let fit = ui
                .add_sized(vec2(28.0, 22.0), egui::Button::new(label))
                .on_hover_text(format!("Along the model's {label} axis"));
            if fit.clicked() {
                (set.from, set.to) = fit_gradient(set.fill, bounds, axis);
            }
        }
        if ui
            .button("Swap")
            .on_hover_text("From and To trade places")
            .clicked()
        {
            std::mem::swap(&mut set.from, &mut set.to);
        }
    });
    ui.add_space(4.0);
    let mut gradient = Gradient {
        name: String::new(),
        dither: Dither::None,
        stops: std::mem::take(&mut set.stops),
        modifiers: Vec::new(),
        ..Gradient::default()
    };
    ui.push_id("color_gradient", |ui| {
        gradient_editor(ui, &mut gradient, card)
    });
    set.stops = gradient.stops;
    // Export; a blending set stays a Color buffer, see docs/features/vertex-colors.md#export.
    if set.blend.is_some() {
        set.export = enrich::ColorExport::Colors;
    }
    ui.add_space(4.0);
    prop_row(ui, "Export", |ui, width| {
        ui.add_enabled_ui(set.blend.is_none(), |ui| {
            egui::ComboBox::from_id_salt("color_export")
                .width(width - 8.0)
                .selected_text(set.export.name())
                .show_ui(ui, |ui| {
                    for export in enrich::ColorExport::ALL {
                        ui.selectable_value(&mut set.export, export, export.name());
                    }
                })
                .response
                .on_disabled_hover_text("Texture + UV buffer needs a set that does not blend");
        });
    });
    clicked
}

/// From and To laying a gradient along `axis` of the model's bounding box; see
/// docs/features/vertex-colors.md#vertex-colors-tab.
fn fit_gradient(
    fill: enrich::ColorFill,
    [min, max]: [bevy::math::Vec3; 2],
    axis: usize,
) -> ([f32; 3], [f32; 3]) {
    let middle = (min + max) / 2.0;
    let (mut from, mut to) = (middle, middle);
    if fill == enrich::ColorFill::Radial {
        to[axis] = max[axis];
    } else {
        (from[axis], to[axis]) = (min[axis], max[axis]);
    }
    (from.to_array(), to.to_array())
}

/// The Regions tab; see docs/features/regions.md#regions-tab.
fn regions_tab(ui: &mut Ui, inspected: &mut Inspected, tools: &mut LocatorTools) -> Option<Amod> {
    let parts = inspected
        .geometry
        .as_ref()
        .map(RegionParts::new)
        .unwrap_or_default();
    let LocatorTools {
        hidden_regions,
        paint,
        rubber,
        brush,
        armed,
        ..
    } = tools;
    let hidden = hidden_regions.entry(inspected.path.clone()).or_default();
    let Some(Ok(edit)) = &mut inspected.mesh else {
        placeholder(ui, "No regions");
        return None;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let room = edit.edit.regions.len() < enrich::MAX_REGIONS;
                let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add region");
                let add = ui
                    .add_enabled(room, add)
                    .on_disabled_hover_text("A model has at most 32 regions");
                if add.clicked() {
                    let regions = &mut edit.edit.regions;
                    let name = enrich::free_name("region", |n| regions.iter().any(|r| r.name == n));
                    let color = LOCATOR_COLORS
                        .iter()
                        .find(|c| regions.iter().all(|r| r.color != **c))
                        .unwrap_or(&LOCATOR_COLORS[0]);
                    regions.push(Region {
                        name,
                        tags: Vec::new(),
                        color: (*color).to_owned(),
                        materials: Vec::new(),
                        submeshes: Default::default(),
                        triangles: Default::default(),
                    });
                    edit.region = Some(regions.len() - 1);
                    *paint = false;
                }
                ui.add_space(8.0);
                if edit.edit.regions.is_empty() {
                    ui.weak("No regions");
                }
                let mut event = None;
                for i in 0..edit.edit.regions.len() {
                    let open = edit.region == Some(i);
                    let name = edit.edit.regions[i].name.clone();
                    let dot = hex_color32(&edit.edit.regions[i].color).unwrap_or(TEXT);
                    let mut visible = !hidden.contains(&name);
                    let mut delete = false;
                    let (_, folded) = ui
                        .push_id(("region", i), |ui| {
                            card(
                                ui,
                                Some(open),
                                RichText::new(&name),
                                |ui| eye_and_dot(ui, &mut visible, dot),
                                |ui, open| {
                                    delete = open
                                        && icon_button(ui, Icon::Trash2, "Delete", true).clicked();
                                },
                                |ui| {
                                    region_fields(ui, edit, i, &parts, paint, rubber, brush, armed)
                                },
                            )
                        })
                        .inner;
                    if folded {
                        event = Some(CardEvent::Fold(i));
                    }
                    if delete {
                        event = Some(CardEvent::Delete(i));
                    }
                    if visible == hidden.contains(&name) {
                        set_visible(hidden, name, visible);
                    }
                }
                match event {
                    Some(CardEvent::Fold(i)) => {
                        edit.region = (edit.region != Some(i)).then_some(i);
                        *paint = false;
                    }
                    Some(CardEvent::Delete(i)) => {
                        edit.edit.regions.remove(i);
                        edit.region = None;
                        *paint = false;
                    }
                    _ => {}
                }
            });
        });
    mesh_commit(ui, edit)
}

/// What a region can cover whole: the model's materials and submeshes.
#[derive(Default)]
struct RegionParts {
    materials: Vec<String>,
    /// glTF mesh and primitive, label and hover text.
    submeshes: Vec<((u32, u32), String, String)>,
}

impl RegionParts {
    fn new(geometry: &crate::model::ModelGeometry) -> Self {
        let submeshes = geometry
            .primitives
            .iter()
            .map(|(mesh, primitive, material, triangles)| {
                let name = geometry
                    .mesh_names
                    .get(*mesh as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("mesh_{mesh}"));
                let hover = format!(
                    "{}, {}",
                    material.as_deref().unwrap_or("no material"),
                    if *triangles == 1 {
                        "1 triangle".to_owned()
                    } else {
                        format!("{triangles} triangles")
                    }
                );
                ((*mesh, *primitive), format!("{name} / {primitive}"), hover)
            })
            .collect();
        Self {
            materials: geometry.materials.clone(),
            submeshes,
        }
    }
}

/// The body of region `i`'s card.
#[allow(clippy::too_many_arguments)]
fn region_fields(
    ui: &mut Ui,
    edit: &mut MeshEdit,
    i: usize,
    parts: &RegionParts,
    paint: &mut bool,
    rubber: &mut bool,
    brush: &mut f32,
    armed: &mut bool,
) {
    let saved = edit.saved.regions.get(i).map(|r| r.name.clone());
    let others: Vec<String> = edit
        .edit
        .regions
        .iter()
        .enumerate()
        .filter(|(k, _)| *k != i)
        .map(|(_, r)| r.name.clone())
        .collect();
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    let region = &mut edit.edit.regions[i];
    name_row(ui, &mut region.name, saved.as_deref(), &others, "region");
    tags_row(ui, &mut region.tags);
    let mut color = Some(region.color.clone());
    color_row(ui, &mut color, false);
    if let Some(color) = color {
        region.color = color;
    }
    prop_row(ui, "Materials", |ui, _| {
        ui.vertical(|ui| {
            if parts.materials.is_empty() {
                ui.weak("None");
            }
            for material in &parts.materials {
                let mut on = region.materials.contains(material);
                if ui.checkbox(&mut on, material).changed() {
                    if on {
                        region.materials.push(material.clone());
                    } else {
                        region.materials.retain(|m| m != material);
                    }
                }
            }
        });
    });
    prop_row(ui, "Submeshes", |ui, _| {
        ui.vertical(|ui| {
            if parts.submeshes.is_empty() {
                ui.weak("None");
            }
            for (id, label, hover) in &parts.submeshes {
                let mut on = region.submeshes.contains(id);
                if ui.checkbox(&mut on, label).on_hover_text(hover).changed() {
                    if on {
                        region.submeshes.insert(*id);
                    } else {
                        region.submeshes.remove(id);
                    }
                }
            }
        });
    });
    prop_row(ui, "Painted", |ui, _| {
        ui.label(format!("{} triangles", region.triangles.len()));
    });
    // Paint and Erase: at most one on; see docs/features/regions.md#regions-tab.
    ui.horizontal(|ui| {
        let tools = [
            (Icon::Paintbrush, "Paint", false),
            (Icon::Eraser, "Erase", true),
        ];
        for (icon, text, erases) in tools {
            let on = *paint && *rubber == erases;
            let mut button = egui::Button::image_and_text(icon.image(14.0, TEXT), text);
            if on {
                button = button.fill(ACCENT.gamma_multiply(0.25));
            }
            if ui.add(button).clicked() {
                (*paint, *rubber) = (!on, erases);
                *armed = false;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().slider_width = 60.0;
        ui.add(
            egui::Slider::new(brush, 2.0..=64.0)
                .fixed_decimals(0)
                .text("px"),
        )
        .on_hover_text("Brush");
        let clear = ui.add_enabled(!region.triangles.is_empty(), egui::Button::new("Clear"));
        if clear.clicked() {
            region.triangles.clear();
        }
    });
    ui.weak("Drag on the model to paint or erase. Shift + drag does the other. Right drag orbits.");
}

/// Three drag fields side by side.
fn vec3_row(
    ui: &mut Ui,
    label: &str,
    values: &mut [f32; 3],
    speed: f64,
    decimals: usize,
    min: Option<f32>,
) {
    prop_row(ui, label, |ui, width| {
        ui.spacing_mut().item_spacing.x = 4.0;
        // Room for the spacing, so long values never widen the panel.
        let each = ((width - 16.0) / 3.0).max(20.0);
        for value in values {
            // One decimal less from 10, two from 100, so long values fit the field.
            let digits = match value.abs() {
                v if v >= 100.0 => 2,
                v if v >= 10.0 => 1,
                _ => 0,
            };
            let shown = if decimals > 1 {
                decimals.saturating_sub(digits).max(1)
            } else {
                decimals
            };
            let mut field = egui::DragValue::new(value)
                .speed(speed)
                .fixed_decimals(shown);
            if let Some(min) = min {
                field = field.range(min..=f32::MAX);
            }
            ui.add_sized(vec2(each, 22.0), field);
        }
    });
}

/// Shows the 3D preview in `rect` and handles orbit input; `orbit` off leaves left drags to
/// something else (right drags still orbit). See docs/features/canvas.md#3d-controls.
fn view_3d(ui: &mut Ui, rect: Rect, preview: &mut Preview3d, orbit: bool) -> Response {
    let ppp = ui.ctx().pixels_per_point();
    let min = (rect.min.to_vec2() * ppp).round();
    let size = (rect.size() * ppp).round().max(Vec2::splat(1.0));
    preview.viewport = Some((
        UVec2::new(min.x as u32, min.y as u32),
        UVec2::new(size.x as u32, size.y as u32),
    ));

    let response = ui.interact(rect, ui.id().with("canvas_3d"), Sense::click_and_drag());
    if response.double_clicked() {
        preview.orbit = Orbit::default();
    } else if (orbit && response.dragged_by(PointerButton::Primary))
        || response.dragged_by(PointerButton::Secondary)
    {
        let delta = response.drag_delta();
        preview.orbit.drag(bevy::math::Vec2::new(delta.x, delta.y));
        preview.dragging = true;
    }
    if response.hovered() {
        let (notches, pinch) = ui.input(|i| (wheel_notches(i), i.zoom_delta()));
        let factor = pinch * WHEEL_ZOOM.powf(notches);
        if factor != 1.0 {
            preview.orbit.dolly(factor);
        }
    }
    orientation_gizmo(ui, rect, &mut preview.orbit);
    response
}

/// World axes as seen from the orbit camera, see docs/features/canvas.md#orientation-gizmo.
fn orientation_gizmo(ui: &Ui, rect: Rect, orbit: &mut Orbit) {
    use bevy::math::Vec3;
    const SIZE: f32 = 64.0;
    const LENGTH: f32 = 22.0;
    let area = Rect::from_min_size(pos2(rect.left() + 8.0, rect.top() + 8.0), vec2(SIZE, SIZE));
    let center = area.center();
    let painter = ui.painter_at(rect);
    painter.circle_filled(center, SIZE / 2.0, BG_BAR.gamma_multiply(0.7));
    let to_view = orbit.transform().rotation.inverse();
    // (screen offset, depth toward the camera, color, letter of a positive axis, axis, sign)
    let mut marks = Vec::new();
    for (index, (axis, color, letter)) in [
        (Vec3::X, CHANNEL_R, "X"),
        (Vec3::Y, CHANNEL_G, "Y"),
        (Vec3::Z, CHANNEL_B, "Z"),
    ]
    .into_iter()
    .enumerate()
    {
        for (sign, letter) in [(1.0, Some(letter)), (-1.0, None)] {
            let v = to_view * (axis * sign);
            marks.push((
                vec2(v.x, -v.y) * LENGTH,
                v.z,
                color,
                letter,
                index,
                sign > 0.0,
            ));
        }
    }
    // Farthest first, so nearer marks cover them.
    marks.sort_by(|a, b| a.1.total_cmp(&b.1));
    // A click on a dot looks along its axis; the nearest dot wins.
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hovered = pointer.and_then(|p| {
        marks
            .iter()
            .map(|m| (m, (center + m.0 - p).length()))
            .filter(|(_, d)| *d <= 8.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(m, _)| (m.4, m.5, center + m.0))
    });
    if let Some((axis, positive, at)) = hovered {
        let sign = if positive { "+" } else { "−" };
        let name = ["X", "Y", "Z"][axis];
        let dot = ui.interact(
            Rect::from_center_size(at, vec2(16.0, 16.0)),
            ui.id().with("orientation_dot"),
            Sense::click(),
        );
        let dot = dot
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!("View from {sign}{name}"));
        if dot.clicked() {
            orbit.look_from(axis, positive);
        }
    }
    for (offset, _, color, letter, axis, positive) in marks {
        let end = center + offset;
        if hovered.is_some_and(|h| (h.0, h.1) == (axis, positive)) {
            let radius = if letter.is_some() { 9.0 } else { 6.0 };
            painter.circle_stroke(end, radius, Stroke::new(1.0, TEXT));
        }
        match letter {
            Some(letter) => {
                painter.line_segment([center, end], Stroke::new(2.0, color));
                painter.circle_filled(end, 7.0, color);
                painter.text(
                    end,
                    Align2::CENTER_CENTER,
                    letter,
                    FontId::proportional(10.0),
                    BG_BAR,
                );
            }
            None => {
                painter.circle_filled(end, 4.0, color.gamma_multiply(0.5));
            }
        }
    }
}

fn shape_tools(current: Shape) -> Vec<Tool> {
    Shape::ALL
        .iter()
        .map(|&shape| Tool {
            glyph: Glyph::Icon(match shape {
                Shape::Box => Icon::Box,
                Shape::Sphere => Icon::Globe,
                Shape::Cylinder => Icon::Cylinder,
                Shape::Torus => Icon::Torus,
            }),
            name: shape.name().into(),
            on: current == shape,
            enabled: true,
        })
        .collect()
}

/// Vertical wheel movement this frame in notches (lines); smooth scrolling counts 50 points
/// as one. Counted from raw events because points per notch differ between platforms.
/// `Ctrl` + wheel is left out: egui already reports it as `zoom_delta`.
fn wheel_notches(input: &egui::InputState) -> f32 {
    input
        .events
        .iter()
        .map(|event| match event {
            egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } if !modifiers.command => match unit {
                egui::MouseWheelUnit::Point => delta.y / 50.0,
                egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page => delta.y,
            },
            _ => 0.0,
        })
        .sum()
}

#[derive(Clone, Copy)]
enum Glyph {
    Icon(Icon),
    /// An eye with a ban sign in the deficiency's color as its pupil.
    Vision(Vision),
    /// Three circles, filled for the shown channels.
    Channels(Channels),
}

struct Tool {
    glyph: Glyph,
    name: String,
    on: bool,
    enabled: bool,
}

/// How compiled meshes are drawn, for the session; see
/// docs/features/models.md#compiled-mesh-preview.
#[derive(Default)]
struct MeshView {
    display: MeshDisplay,
    /// Bounds drawn over the mesh, by index in [`MeshOverlay::ALL`].
    overlays: [bool; 6],
    /// The file's locators are not drawn; see docs/features/locators.md#compiled-mesh-preview.
    hide_locators: bool,
}

/// The Color vision group: at most one on; see docs/features/canvas.md#color-vision.
fn vision_tools(current: Option<Vision>) -> Vec<Tool> {
    Vision::ALL
        .into_iter()
        .map(|seen| Tool {
            glyph: Glyph::Vision(seen),
            name: format!("{}\n{}", seen.name(), seen.hint()),
            on: current == Some(seen),
            enabled: true,
        })
        .collect()
}

/// Turns button `button` of the Color vision group on, or off when it already is.
fn toggle_vision(vision: &mut Option<Vision>, button: usize) {
    let seen = Vision::ALL[button];
    *vision = (*vision != Some(seen)).then_some(seen);
}

/// A column of button groups in the canvas' top-right corner; returns the clicked `(group, button)`.
fn toolbar(ui: &mut Ui, canvas: Rect, groups: &[&[Tool]]) -> Option<(usize, usize)> {
    const BUTTON: Vec2 = vec2(36.0, 32.0);
    const RADIUS: u8 = 6;
    const GAP: f32 = 8.0;
    let painter = ui.painter_at(canvas);
    let x = canvas.right() - 8.0 - BUTTON.x;
    let mut y = canvas.top() + 8.0;
    let mut clicked = None;
    for (g, tools) in groups.iter().enumerate() {
        let bar = Rect::from_min_size(pos2(x, y), vec2(BUTTON.x, BUTTON.y * tools.len() as f32));
        y = bar.bottom() + GAP;
        painter.rect_filled(bar, RADIUS, BG_BAR);
        let last = tools.len() - 1;
        for (i, tool) in tools.iter().enumerate() {
            let rect = Rect::from_min_size(bar.min + vec2(0.0, BUTTON.y * i as f32), BUTTON);
            // Disabled buttons still take the pointer so it never reaches the canvas.
            let sense = if tool.enabled {
                Sense::click_and_drag()
            } else {
                Sense::drag()
            };
            let response = ui
                .interact(rect, ui.id().with(("toolbar", g, i)), sense)
                .on_hover_text(&tool.name);
            // Round only the corners on the group's outer edge.
            let outer = |edge: bool| if edge { RADIUS } else { 0 };
            let radius = egui::CornerRadius {
                nw: outer(i == 0),
                ne: outer(i == 0),
                sw: outer(i == last),
                se: outer(i == last),
            };
            let tint = if tool.on {
                painter.rect_filled(rect, radius, ACCENT);
                BG_BAR
            } else if !tool.enabled {
                TEXT_WEAK.gamma_multiply(0.5)
            } else if response.hovered() {
                painter.rect_filled(rect, radius, BG_HEADER);
                TEXT
            } else {
                TEXT_WEAK
            };
            if i > 0 {
                painter.hline(rect.x_range(), rect.top(), Stroke::new(1.0, STROKE));
            }
            paint_glyph(ui, &painter, tool, rect.center(), tint);
            if response.clicked() {
                clicked = Some((g, i));
            }
        }
    }
    clicked
}

fn paint_glyph(
    ui: &Ui,
    painter: &egui::Painter,
    tool: &Tool,
    center: egui::Pos2,
    tint: egui::Color32,
) {
    // Disabled buttons show their fill colors dimmed too.
    let fill = |color: egui::Color32| {
        if tool.enabled {
            color
        } else {
            color.gamma_multiply(0.35)
        }
    };
    match tool.glyph {
        Glyph::Icon(icon) => icon
            .image(18.0, tint)
            .paint_at(ui, Rect::from_center_size(center, vec2(18.0, 18.0))),
        Glyph::Vision(seen) => {
            Icon::EyeOutline
                .image(18.0, tint)
                .paint_at(ui, Rect::from_center_size(center, vec2(18.0, 18.0)));
            let color = fill(match seen {
                Vision::Protanopia => CHANNEL_R,
                Vision::Deuteranopia => CHANNEL_G,
                Vision::Tritanopia => CHANNEL_B,
                Vision::Monochromacy => egui::Color32::from_rgb(0x8c, 0x8c, 0x8c),
            });
            let stroke = Stroke::new(1.5, color);
            painter.circle_stroke(center, 3.5, stroke);
            let d = 3.5 * std::f32::consts::FRAC_1_SQRT_2;
            painter.line_segment([center - vec2(d, d), center + vec2(d, d)], stroke);
        }
        Glyph::Channels(shown) => {
            // Red top, green bottom-left, blue bottom-right; see docs/features/canvas.md#channel-icons.
            let circles = [
                (vec2(0.0, -3.2), Channels::R, CHANNEL_R),
                (vec2(-3.7, 2.8), Channels::G, CHANNEL_G),
                (vec2(3.7, 2.8), Channels::B, CHANNEL_B),
            ];
            // circle-small is r = 6 of 24, so 4 px at 16 px.
            // Gray: black, gray, white instead of the channel colors.
            let gray = [egui::Color32::BLACK, TEXT_WEAK, egui::Color32::WHITE];
            for (i, (offset, channel, color)) in circles.into_iter().enumerate() {
                match shown {
                    Channels::Gray => {
                        painter.circle_filled(center + offset, 4.0, fill(gray[i]));
                    }
                    Channels::Rgb => {
                        painter.circle_filled(center + offset, 4.0, fill(color));
                    }
                    _ if shown == channel => {
                        painter.circle_filled(center + offset, 4.0, fill(color));
                    }
                    _ => {}
                }
            }
            for (offset, ..) in circles {
                Icon::CircleSmall.image(16.0, tint).paint_at(
                    ui,
                    Rect::from_center_size(center + offset, vec2(16.0, 16.0)),
                );
            }
        }
    }
}

/// Multiplies the zoom by `factor`, keeping the image point at `anchor`
/// (relative to the canvas center) in place.
fn zoom_at(view: &mut View, anchor: Vec2, factor: f32) {
    let zoom = (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let ratio = zoom / view.zoom;
    view.pan = anchor - (anchor - view.pan) * ratio;
    view.zoom = zoom;
    view.fit = false;
}

/// Checkerboard under the visible part of `image`, cells anchored to the image corner.
fn checkerboard(painter: &egui::Painter, image: Rect) {
    const CELL: f32 = 8.0;
    let visible = image.intersect(painter.clip_rect());
    if !visible.is_positive() {
        return;
    }
    painter.rect_filled(visible, 0.0, CHECKER_DARK);
    let first = ((visible.min - image.min) / CELL).floor();
    let last = ((visible.max - image.min) / CELL).ceil();
    for y in first.y as i64..last.y as i64 {
        for x in first.x as i64..last.x as i64 {
            if (x + y) % 2 == 0 {
                continue;
            }
            let min = image.min + vec2(x as f32, y as f32) * CELL;
            let cell = Rect::from_min_size(min, Vec2::splat(CELL)).intersect(visible);
            painter.rect_filled(cell, 0.0, CHECKER_LIGHT);
        }
    }
}

/// `[ − ][ 100% ][ + ][ Fit ]` in the canvas' bottom-right corner.
/// Returns whether a button changed the view.
fn zoom_bar(ui: &mut Ui, canvas: Rect, view: &mut View, fit: impl Fn(&mut View)) -> bool {
    const HEIGHT: f32 = 28.0;
    let font = FontId::proportional(13.0);
    let labels = [
        "−".to_string(),
        format!("{:.0}%", view.zoom * 100.0),
        "+".to_string(),
        "Fit".to_string(),
    ];
    let widths = labels.each_ref().map(|label| {
        ui.painter()
            .layout_no_wrap(label.clone(), font.clone(), TEXT)
            .size()
            .x
            + 16.0
    });
    let total: f32 = widths.iter().sum();
    let bar = Rect::from_min_size(
        canvas.right_bottom() - vec2(total + 8.0, HEIGHT + 8.0),
        vec2(total, HEIGHT),
    );
    let painter = ui.painter_at(canvas);
    painter.rect(
        bar,
        0.0,
        BG_BAR,
        Stroke::new(1.0, STROKE),
        StrokeKind::Inside,
    );
    let mut changed = false;
    let mut x = bar.left();
    for (i, (label, width)) in labels.iter().zip(widths).enumerate() {
        let rect = Rect::from_min_size(pos2(x, bar.top()), vec2(width, HEIGHT));
        x += width;
        // Covers the whole bar so scrolling or dragging on it never reaches the canvas.
        let response = ui.interact(rect, ui.id().with(("zoom_bar", i)), Sense::click_and_drag());
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 0.0, BG_HEADER);
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            font.clone(),
            TEXT,
        );
        if response.clicked() {
            match i {
                0 => zoom_at(view, Vec2::ZERO, std::f32::consts::FRAC_1_SQRT_2),
                1 => zoom_at(view, Vec2::ZERO, 1.0 / view.zoom),
                2 => zoom_at(view, Vec2::ZERO, std::f32::consts::SQRT_2),
                _ => fit(view),
            }
            changed = true;
        }
    }
    changed
}

fn section_header(ui: &mut Ui, title: &str) {
    Frame::new()
        .fill(BG_HEADER)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong());
        });
}

fn placeholder(ui: &mut Ui, text: &str) {
    Frame::new()
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.weak(text);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_land_on_hundredths() {
        let close = |a: f32, b: f32| (a - b).abs() < 1e-5;
        assert!(close(step_time(0.304, 0.01, false), 0.30));
        assert!(close(step_time(0.304, 0.01, true), 0.31));
        assert!(close(step_time(0.30, 0.01, false), 0.29));
        assert!(close(step_time(0.30, 0.01, true), 0.31));
        // A time a hair under a hundredth counts as on it.
        assert!(close(step_time(0.29999, 0.01, false), 0.29));
        assert!(close(step_time(0.29999, 0.01, true), 0.31));
        // Thousandths with Shift.
        assert!(close(step_time(0.3045, 0.001, false), 0.304));
        assert!(close(step_time(0.3045, 0.001, true), 0.305));
        assert!(close(step_time(0.305, 0.001, true), 0.306));
    }
}
