use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{
    Align, Align2, FontId, Frame, Layout, Margin, PointerButton, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use crate::assets::Assets;
use crate::build::{self, Outcome};
use crate::config::Project;
use crate::fileops;
use crate::icons::Icon;
use crate::inspect::{self, Inspection, Location};
use crate::logo;
use crate::lut::Lut;
use crate::manifest::Kind;
use crate::noise::{
    GeneratorKind, Modifier, ModifierKind, Noise, ParamKind, ParamSpec, Params, Value,
};
use crate::preview3d::{CubemapUpload, Orbit, Preview3d, Shape, Upload};
use crate::texture::Image;
use crate::theme::{
    self, ACCENT, BG_BAR, BG_CENTER, BG_HEADER, BG_PANEL, CHANNEL_B, CHANNEL_G, CHANNEL_R,
    CHECKER_DARK, CHECKER_LIGHT, STROKE, TEXT, TEXT_WEAK,
};
use crate::tree::{FileTrees, Listing, Node};
use crate::{chart, preview3d};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let [r, g, b, _] = BG_CENTER.to_array();
        app.insert_resource(ClearColor(Color::srgb_u8(r, g, b)))
            .init_resource::<UiState>()
            .add_systems(Startup, (scan_trees, set_title))
            .add_systems(EguiPrimaryContextPass, shell);
    }
}

#[derive(Resource)]
struct UiState {
    show_left_sidebar: bool,
    show_right_sidebar: bool,
    inspector_tab: InspectorTab,
    browser_tab: BrowserTab,
    rescan: bool,
    start_build: bool,
    build: BuildStatus,
    selection: Option<Selection>,
    /// Set when the selection changed or the trees were rescanned.
    reinspect: bool,
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
    /// Per `.noise` file: parameters of generator types switched away from this session.
    noise_memory: HashMap<PathBuf, HashMap<GeneratorKind, Params>>,
    /// Canvas toolbar state of each image, by path.
    canvas_options: HashMap<PathBuf, CanvasOptions>,
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
    preview: Option<egui::TextureHandle>,
    /// Single-channel view of `pixels`, built on demand.
    channel_preview: Option<(Channels, egui::TextureHandle)>,
    /// Channels last sent to the 3D preview.
    uploaded_3d: Option<Channels>,
    /// Environment map faces, kept to upload to the 3D preview.
    cubemap: Option<Vec<Image>>,
    uploaded_cubemap: bool,
    lut: Option<Lut>,
    /// A `.noise` file's parameters: (being edited, as saved), or why they can't be read.
    noise: Option<Result<NoiseEdit, String>>,
    lut_compare: Option<CompareCache>,
    uploaded_color_cube: bool,
}

/// Before / after textures of the LUT compare view for one source image.
struct CompareCache {
    /// Path of the source image; `None` for the reference chart.
    key: Option<PathBuf>,
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
    Running(JoinHandle<Result<Outcome, build::Error>>),
    Finished(String),
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_left_sidebar: true,
            show_right_sidebar: true,
            inspector_tab: InspectorTab::Info,
            browser_tab: BrowserTab::Input,
            rescan: false,
            start_build: false,
            build: BuildStatus::Idle,
            selection: None,
            reinspect: false,
            inspected: None,
            view: View::default(),
            mode: ViewMode::default(),
            lut: LutUi::default(),
            last_image: None,
            open: HashSet::new(),
            rename: None,
            confirm_delete: None,
            notice: None,
            noise_memory: HashMap::new(),
            canvas_options: HashMap::new(),
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
    Modifiers,
    Assets,
    Settings,
}

impl InspectorTab {
    const ALL: [Self; 5] = [
        Self::Info,
        Self::Parameters,
        Self::Modifiers,
        Self::Assets,
        Self::Settings,
    ];

    fn icon(self) -> Icon {
        match self {
            Self::Info => Icon::Info,
            Self::Parameters => Icon::SlidersVertical,
            Self::Modifiers => Icon::Wrench,
            Self::Assets => Icon::Files,
            Self::Settings => Icon::Settings,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Parameters => "Parameters",
            Self::Modifiers => "Modifiers",
            Self::Assets => "Assets",
            Self::Settings => "Settings",
        }
    }

    fn enabled(self, selection: Option<&Selection>) -> bool {
        match self {
            Self::Info | Self::Settings => true,
            Self::Parameters | Self::Modifiers => selection.is_some_and(|s| {
                s.location == Location::Input && build::is_noise(&s.path) && s.path.is_file()
            }),
            // Only compiled files are asset containers for now.
            Self::Assets => selection.is_some_and(|s| {
                s.location == Location::Output
                    && s.path.is_file()
                    && s.path.extension().is_some_and(|ext| {
                        Kind::ALL
                            .iter()
                            .any(|kind| ext.eq_ignore_ascii_case(kind.extension()))
                    })
            }),
        }
    }
}

fn scan_trees(mut commands: Commands, project: Res<Project>) {
    commands.insert_resource(FileTrees::scan(&project));
    commands.insert_resource(Assets::load(&project));
}

fn set_title(project: Res<Project>, mut window: Single<&mut Window>) {
    if let Some(name) = project.root.as_deref().and_then(|r| r.file_name()) {
        window.title = format!("assetc — {}", name.to_string_lossy());
    }
}

#[allow(clippy::too_many_arguments)]
fn shell(
    mut contexts: EguiContexts,
    mut state: ResMut<UiState>,
    project: Res<Project>,
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
        .show(&mut root, |ui| menu_bar(ui, logo, &mut state, &mut exit));

    egui::Panel::bottom("status_bar")
        .frame(bar_frame())
        .show(&mut root, |ui| {
            status_bar(ui, &project, &state.build, state.notice.as_deref())
        });

    if state.show_left_sidebar {
        egui::Panel::left("left_sidebar")
            .resizable(true)
            .default_size(260.0)
            .min_size(200.0)
            .frame(Frame::new().fill(BG_PANEL))
            .show(&mut root, |ui| {
                left_sidebar(ui, &mut state, &project, &trees)
            });
    }

    if state.show_right_sidebar {
        egui::Panel::right("right_sidebar")
            .resizable(true)
            .default_size(320.0)
            .min_size(220.0)
            .frame(Frame::new().fill(BG_PANEL))
            .show(&mut root, |ui| right_sidebar(ui, &mut state, &assets));
    }

    // In 3D the canvas is left transparent so the 3D camera's image shows through.
    preview3d.viewport = None;
    preview3d.dragging = false;
    preview3d.environment = false;
    preview3d.colors = false;
    let shown = Shown::of(state.inspected.as_ref());
    let show_3d = match shown {
        Shown::Image => state.mode == ViewMode::ThreeD,
        Shown::Environment => true,
        Shown::Lut => state.lut.view == LutView::Cube,
        Shown::Other => false,
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
            let full = ui.max_rect();
            let bar = Rect::from_min_size(full.min, vec2(full.width(), 36.0));
            let modes = ViewMode::ALL.map(ViewMode::name);
            let mode = ViewMode::ALL.iter().position(|m| *m == state.mode);
            let lut_views = LutView::ALL.map(LutView::name);
            let lut_view = LutView::ALL.iter().position(|v| *v == state.lut.view);
            // Only images have a 2D/3D choice; an environment map only exists in 3D.
            let (options, active, enabled): (&[&str], _, _) = match shown {
                Shown::Image => (&modes, mode, true),
                Shown::Lut => (&lut_views, lut_view, true),
                Shown::Environment => (&[], None, false),
                Shown::Other => (&modes, None, false),
            };
            if let Some(i) = view_bar(ui, bar, options, active, enabled) {
                match shown {
                    Shown::Lut => state.lut.view = LutView::ALL[i],
                    _ => state.mode = ViewMode::ALL[i],
                }
            }
            let rect = Rect::from_min_max(pos2(full.left(), bar.bottom()), full.max);
            match (shown, state.mode) {
                (Shown::Environment, _) => {
                    canvas_environment(ui, rect, state.inspected.as_mut(), &mut preview3d)
                }
                (Shown::Lut, _) => canvas_lut(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut state.lut,
                    state.last_image.as_ref(),
                    &mut preview3d,
                ),
                (_, ViewMode::TwoD) => canvas(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut state.view,
                    &mut state.canvas_options,
                ),
                (_, ViewMode::ThreeD) => canvas_3d(
                    ui,
                    rect,
                    state.inspected.as_mut(),
                    &mut state.canvas_options,
                    &mut preview3d,
                ),
            }
        });

    confirm_delete(&ctx, &mut state, &project);

    if std::mem::take(&mut state.start_build) {
        state.notice = None;
        start_build(&mut state, &project);
    }
    poll_build(&mut state);

    if std::mem::take(&mut state.rescan) {
        *trees = FileTrees::scan(&project);
        *assets = Assets::load(&project);
        state.reinspect = true;
    }
    if std::mem::take(&mut state.reinspect) {
        state.inspected = state
            .selection
            .as_ref()
            .map(|selection| inspect_selection(&ctx, &project, selection));
        // Remembered as the LUT compare view's "Image" source.
        if let Some(inspected) = &state.inspected
            && let Some(pixels) = &inspected.pixels
        {
            state.last_image = Some(LastImage {
                path: inspected.path.clone(),
                name: inspected.inspection.name.clone(),
                image: pixels.clone(),
            });
        }
        ctx.request_repaint();
    }

    Ok(())
}

fn inspect_selection(ctx: &egui::Context, project: &Project, selection: &Selection) -> Inspected {
    let mut inspection = inspect::inspect(project, selection.location, &selection.path);
    let cubemap = inspection.cubemap.take();
    let lut = inspection.lut.take();
    let noise =
        (selection.location == Location::Input && build::is_noise(&selection.path)).then(|| {
            std::fs::read_to_string(&selection.path)
                .map_err(|e| e.to_string())
                .and_then(|text| Noise::parse(&text))
                .map(|n| NoiseEdit {
                    edit: n.clone(),
                    saved: n,
                })
        });
    let max_side = ctx.input(|i| i.max_texture_side) as u32;
    let pixels = inspection
        .preview
        .take()
        .map(|image| inspect::fit_within(image, max_side));
    let preview = pixels
        .as_ref()
        .map(|image| load_preview(ctx, "canvas_preview", image, &image.pixels));
    Inspected {
        path: selection.path.clone(),
        inspection,
        pixels,
        preview,
        channel_preview: None,
        uploaded_3d: None,
        cubemap,
        uploaded_cubemap: false,
        lut,
        noise,
        lut_compare: None,
        uploaded_color_cube: false,
    }
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

fn start_build(state: &mut UiState, project: &Project) {
    if matches!(state.build, BuildStatus::Running(_)) {
        return;
    }
    let (Some(input), Some(output)) = (project.input_dir(), project.output_dir()) else {
        return;
    };
    let textures = project.config.textures.clone();
    state.build = BuildStatus::Running(std::thread::spawn(move || {
        build::run(&input, &output, &textures)
    }));
}

fn poll_build(state: &mut UiState) {
    let BuildStatus::Running(handle) = &state.build else {
        return;
    };
    if !handle.is_finished() {
        return;
    }
    let BuildStatus::Running(handle) = std::mem::replace(&mut state.build, BuildStatus::Idle)
    else {
        unreachable!();
    };
    let message = match handle.join() {
        Ok(Ok(outcome)) => outcome.summary(),
        Ok(Err(e)) => format!("Build failed: {e}"),
        Err(_) => "Build failed: build thread panicked".into(),
    };
    state.build = BuildStatus::Finished(message);
    state.rescan = true;
}

fn bar_frame() -> Frame {
    Frame::new()
        .fill(BG_BAR)
        .inner_margin(Margin::symmetric(8, 4))
        .stroke(Stroke::new(1.0, STROKE))
}

fn menu_bar(
    ui: &mut Ui,
    logo: &egui::TextureHandle,
    state: &mut UiState,
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
        ui.menu_button("File", |ui| {
            let response = menu_row(ui, "Quit", None, true);
            ui.painter().text(
                pos2(response.rect.right() - 12.0, response.rect.center().y),
                Align2::RIGHT_CENTER,
                ui.ctx().format_shortcut(&quit),
                FontId::proportional(14.0),
                TEXT_WEAK,
            );
            if response.clicked() {
                exit.write(AppExit::Success);
            }
        });
        ui.add_enabled_ui(false, |ui| {
            ui.menu_button("Edit", |_| {});
        });
        ui.menu_button("View", |ui| {
            let left = &mut state.show_left_sidebar;
            if menu_item(ui, "Left Sidebar", Some(*left)).clicked() {
                *left = !*left;
            }
            let right = &mut state.show_right_sidebar;
            if menu_item(ui, "Right Sidebar", Some(*right)).clicked() {
                *right = !*right;
            }
        });
        ui.menu_button("Help", |ui| {
            menu_row(
                ui,
                &format!("assetc {}", env!("CARGO_PKG_VERSION")),
                None,
                false,
            );
        });
    });
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

fn status_bar(ui: &mut Ui, project: &Project, build: &BuildStatus, notice: Option<&str>) {
    ui.horizontal(|ui| {
        match build {
            _ if let Some(notice) = notice => ui.label(notice),
            BuildStatus::Running(_) => ui.label("Building…"),
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
        if icon_button(ui, Icon::RefreshCw, "Rescan", true).clicked() {
            state.rescan = true;
        }
        let can_build = project.root.is_some() && !matches!(state.build, BuildStatus::Running(_));
        if icon_button(ui, Icon::Hammer, "Build", can_build).clicked() {
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
            let mut cx = TreeCx {
                location,
                selection: state.selection.as_ref(),
                open: &mut state.open,
                editable,
                rename: &mut state.rename,
                rename_done: None,
                action: None,
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
            // F2 / Delete act on the selected Input entry.
            let keys_free = editable && !ui.ctx().egui_wants_keyboard_input();
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
fn icon_tab(ui: &mut Ui, icon: Icon, selected: bool, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 36.0), sense);
    if selected {
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, BG_PANEL);
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            ACCENT,
        );
    }
    let color = if enabled {
        tab_color(selected, response.hovered())
    } else {
        TEXT_WEAK.gamma_multiply(0.5)
    };
    icon.image(18.0, color)
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

/// The grip a modifier card is dragged by, see docs/features/noise.md#modifiers-tab.
fn drag_handle(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::drag());
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
        if !state.inspector_tab.enabled(state.selection.as_ref()) {
            state.inspector_tab = InspectorTab::Info;
        }
    }
}

/// A file operation requested from the Input tree, see docs/features/file-operations.md.
enum FileOp {
    NewFolder(PathBuf),
    NewNoise(PathBuf, GeneratorKind),
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
}

/// Menu items creating something in `dir`.
fn create_items(ui: &mut Ui, dir: &Path, action: &mut Option<TreeAction>) {
    if menu_item(ui, "New Folder", None).clicked() {
        *action = Some(TreeAction::Op(FileOp::NewFolder(dir.to_path_buf())));
        ui.close();
    }
    // Same size, color and text inset as `menu_row`.
    ui.spacing_mut().button_padding = vec2(24.0, 7.0);
    ui.menu_button(RichText::new("New Noise").size(14.0).color(TEXT), |ui| {
        for kind in GeneratorKind::ALL {
            if menu_item(ui, kind.name(), None).clicked() {
                *action = Some(TreeAction::Op(FileOp::NewNoise(dir.to_path_buf(), kind)));
                ui.close();
            }
        }
    });
}

fn tree(ui: &mut Ui, nodes: &[Node], depth: usize, cx: &mut TreeCx) {
    for node in nodes {
        let is_dir = node.children.is_some();
        let open = cx.open.contains(&node.path);
        let (chevron, icon) = match (is_dir, open) {
            (true, true) => (Some(Icon::ChevronDown), Icon::FolderOpen),
            (true, false) => (Some(Icon::ChevronRight), Icon::Folder),
            (false, _) => (None, Icon::File),
        };
        let selected = cx
            .selection
            .is_some_and(|s| s.location == cx.location && s.path == node.path);
        let renaming = cx.rename.as_mut().filter(|r| r.path == node.path);
        let (response, done) = tree_row(ui, depth, chevron, icon, &node.name, selected, renaming);
        if done.is_some() {
            cx.rename_done = done;
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
        if let Some(children) = &node.children
            && open
        {
            tree(ui, children, depth + 1, cx);
        }
    }
}

/// A tree row; while `renaming`, its name is a text field. Returns the row response and,
/// when the rename ends this frame, whether to commit it.
fn tree_row(
    ui: &mut Ui,
    depth: usize,
    chevron: Option<Icon>,
    icon: Icon,
    name: &str,
    selected: bool,
    renaming: Option<&mut Rename>,
) -> (Response, Option<bool>) {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.25));
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(2.0, rect.height())),
            0.0,
            ACCENT,
        );
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
            TEXT,
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

fn right_sidebar(ui: &mut Ui, state: &mut UiState, assets: &Assets) {
    section_header(ui, "Inspector");

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
                let enabled = tab.enabled(state.selection.as_ref());
                let response = icon_tab(ui, tab.icon(), state.inspector_tab == tab, enabled);
                if response.on_hover_text(tab.name()).clicked() {
                    state.inspector_tab = tab;
                }
            }
        });

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_PANEL))
        .show(ui, |ui| {
            match (state.inspector_tab, &mut state.inspected, &state.selection) {
                (_, None, _) | (_, _, None) => placeholder(ui, "Nothing selected"),
                (
                    tab @ (InspectorTab::Parameters | InspectorTab::Modifiers),
                    Some(inspected),
                    Some(selection),
                ) => {
                    let save = if tab == InspectorTab::Parameters {
                        let others = state
                            .noise_memory
                            .entry(selection.path.clone())
                            .or_default();
                        parameters_tab(ui, inspected, others)
                    } else {
                        modifiers_tab(ui, inspected)
                    };
                    if let Some(noise) = save {
                        match std::fs::write(&selection.path, noise.to_toml()) {
                            Ok(()) => state.reinspect = true,
                            Err(e) => state.notice = Some(format!("Saving failed: {e}")),
                        }
                    }
                }
                (InspectorTab::Info, Some(inspected), _) => info(ui, &inspected.inspection),
                (InspectorTab::Assets, _, Some(selection)) => {
                    assets_tab(ui, assets, &selection.path)
                }
                (InspectorTab::Settings, ..) => placeholder(ui, "No settings for this file"),
            }
        });
}

/// A `.noise` file being edited in the Parameters / Modifiers tabs.
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

/// A section card, see docs/features/gui.md#property-editor. When `collapsible`, a chevron
/// starts the header; `leading` draws next (before the title), `trailing` on the right.
#[allow(clippy::too_many_arguments)]
fn prop_section(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    collapsible: bool,
    title: RichText,
    leading: impl FnOnce(&mut Ui),
    trailing: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) -> Rect {
    let id = ui.make_persistent_id(id);
    let open = !collapsible || ui.data(|d| d.get_temp::<bool>(id).unwrap_or(true));
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
                    if chevron.is_some_and(|c| c.clicked()) || (collapsible && title.clicked()) {
                        ui.data_mut(|d| d.insert_temp(id, !open));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add_space(4.0);
                        trailing(ui);
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
    card.response.rect
}

/// A row: `label` right-aligned in the label column, then `control` given the rest of the width.
fn prop_row(ui: &mut Ui, label: &str, control: impl FnOnce(&mut Ui, f32)) {
    let width = ui.available_width();
    let label_width = (width * 0.4).max(80.0);
    ui.allocate_ui_with_layout(
        vec2(width, 26.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (rect, _) = ui.allocate_exact_size(vec2(label_width - 8.0, 26.0), Sense::hover());
            ui.painter().text(
                rect.right_center(),
                Align2::RIGHT_CENTER,
                label,
                FontId::proportional(13.0),
                TEXT_WEAK,
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
        (ParamKind::Color { .. }, Value::Color(rgb)) => {
            let mut srgb = rgb.map(|c| (c * 255.0).round() as u8);
            if ui.color_edit_button_srgb(&mut srgb).changed() {
                *rgb = srgb.map(|c| c as f64 / 255.0);
            }
        }
        _ => {
            ui.weak("—");
        }
    });
}

/// Generator and its parameters; see docs/features/noise.md#parameters-tab.
/// `others` holds the file's parameters of other generator types set this session; saving
/// re-inspects the file, so they live outside [`Inspected`].
fn parameters_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    others: &mut HashMap<GeneratorKind, Params>,
) -> Option<Noise> {
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
                    true,
                    RichText::new("Generator"),
                    |_| {},
                    |_| {},
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
                if kind != noise.kind {
                    // Each type keeps its own parameters for the session.
                    let params = others
                        .remove(&kind)
                        .unwrap_or_else(|| Params::defaults(kind.specs()));
                    let old = std::mem::replace(&mut noise.params, params);
                    others.insert(noise.kind, old);
                    noise.kind = kind;
                }
                let specs = noise.kind.specs();
                prop_section(
                    ui,
                    ("generator_params", noise.kind),
                    true,
                    RichText::new(noise.kind.name()),
                    |_| {},
                    |_| {},
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

enum ModifierAction {
    Toggle(usize),
    Remove(usize),
}

/// The modifier list; see docs/features/noise.md#modifiers-tab.
fn modifiers_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Noise> {
    let edit = noise_edit(ui, inspected)?;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                let modifiers = &mut edit.edit.modifiers;
                // Set from the header closures, applied after the list is drawn.
                let action = std::cell::Cell::new(None);
                let dragging = std::cell::Cell::new(None);
                let dropped = std::cell::Cell::new(None);
                let mut cards = Vec::with_capacity(modifiers.len());
                for (i, m) in modifiers.iter_mut().enumerate() {
                    let enabled = m.enabled;
                    let title =
                        RichText::new(m.kind.name()).color(if enabled { TEXT } else { TEXT_WEAK });
                    cards.push(prop_section(
                        ui,
                        ("modifier", i, m.kind.key()),
                        false,
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
                        |ui| {
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
                if let Some(from) = dragging.get().or(dropped.get()) {
                    let pointer = ui.ctx().pointer_latest_pos().map_or(0.0, |p| p.y);
                    let to = cards.iter().filter(|r| r.center().y < pointer).count();
                    if dropped.get().is_some() {
                        if to != from && to != from + 1 {
                            let m = modifiers.remove(from);
                            modifiers.insert(if to > from { to - 1 } else { to }, m);
                        }
                    } else {
                        let painter = ui.painter();
                        painter.rect_stroke(
                            cards[from],
                            6,
                            Stroke::new(1.0, ACCENT),
                            egui::StrokeKind::Inside,
                        );
                        if to != from && to != from + 1 {
                            // In the middle of the 8 px gap above card `to` (or below the last).
                            let y = cards
                                .get(to)
                                .map_or(cards[to - 1].bottom() + 4.0, |r| r.top() - 4.0);
                            let rect = cards[from];
                            painter.hline(rect.x_range(), y, Stroke::new(2.0, ACCENT));
                        }
                    }
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
                let button =
                    egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add modifier");
                egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                    for kind in ModifierKind::ALL {
                        if menu_item(ui, kind.name(), None).clicked() {
                            modifiers.push(Modifier::new(kind));
                            ui.close();
                        }
                    }
                });
            });
        });
    noise_commit(ui, edit)
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

fn assets_tab(ui: &mut Ui, assets: &Assets, file: &std::path::Path) {
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
                        };
                        ui.add(icon.image(16.0, TEXT_WEAK));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&asset.asset_path).strong());
                            ui.horizontal(|ui| {
                                ui.weak(asset.kind.name());
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
    Environment,
    Lut,
    Other,
}

impl Shown {
    fn of(inspected: Option<&Inspected>) -> Self {
        match inspected {
            Some(i) if i.cubemap.is_some() => Self::Environment,
            Some(i) if i.lut.is_some() => Self::Lut,
            Some(i) if i.preview.is_some() && i.pixels.is_some() => Self::Image,
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
) {
    let Some(inspected) = shown_image(ui, rect, inspected) else {
        return;
    };
    let options = image_options(inspected, all_options);
    let (Some(preview), Some(pixels)) = (&inspected.preview, &inspected.pixels) else {
        return;
    };
    let texture = if options.channels.index().is_none() {
        preview.clone()
    } else {
        let cached = &mut inspected.channel_preview;
        if cached.as_ref().is_none_or(|(c, _)| *c != options.channels) {
            let gray = channel_pixels(pixels, options.channels);
            let texture = load_preview(ui.ctx(), "canvas_channel", pixels, &gray);
            *cached = Some((options.channels, texture));
        }
        cached.as_ref().unwrap().1.clone()
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

    // Drawn last so they sit on top; their clicks apply next frame.
    let tile = [Tool {
        glyph: Glyph::Icon(Icon::Grid3x3),
        name: "Tile".into(),
        on: options.tiled,
        enabled: true,
    }];
    let choices = channel_choices(&inspected.inspection);
    let channels = channel_tools(&choices, options.channels);
    if let Some((group, button)) = toolbar(ui, rect, &[&tile, &channels]) {
        let options = all_options.entry(inspected.path.clone()).or_default();
        match group {
            0 => options.tiled = !options.tiled,
            _ => options.channels = choices[button].0,
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
    view_3d(ui, rect, preview);

    let shapes = shape_tools(preview.shape);
    let choices = channel_choices(&inspected.inspection);
    let channels = channel_tools(&choices, options.channels);
    if let Some((group, button)) = toolbar(ui, rect, &[&shapes, &channels]) {
        match group {
            0 => preview.shape = Shape::ALL[button],
            _ => {
                let options = all_options.entry(inspected.path.clone()).or_default();
                options.channels = choices[button].0;
            }
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
        LutView::Compare => compare_view(ui, rect, lut, lut_compare, lut_ui, last_image),
        LutView::Cube => {
            if !*uploaded_color_cube {
                let before = preview3d::color_cube(lut.size, |rgb| rgb);
                let after = preview3d::color_cube(lut.size, |rgb| lut.apply(rgb));
                preview.color_cube = Some((before, after));
                *uploaded_color_cube = true;
            }
            preview.colors = true;
            view_3d(ui, rect, preview);
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
            if let Some((_, button)) = toolbar(ui, rect, &[&tools]) {
                lut_ui.cube_before = button == 0;
                ui.ctx().request_repaint();
            }
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
) {
    let (key, image) = match (lut_ui.source, last_image) {
        (LutSource::Image, Some(last)) => (Some(&last.path), &last.image),
        _ => (None, &*chart::CHART),
    };
    if cache.as_ref().is_none_or(|c| c.key.as_ref() != key) {
        let graded = lut.grade(image);
        let linear = egui::TextureFilter::Linear;
        *cache = Some(CompareCache {
            key: key.cloned(),
            before: load_texture(ui.ctx(), "lut_before", image, &image.pixels, linear),
            after: load_texture(ui.ctx(), "lut_after", &graded, &graded.pixels, linear),
        });
    }
    let cache = cache.as_ref().unwrap();
    let shown = fit_rect(rect, cache.before.size_vec2());

    let response = ui.interact(rect, ui.id().with("lut_compare"), Sense::click_and_drag());
    if (response.clicked() || response.dragged_by(PointerButton::Primary))
        && let Some(pointer) = response.interact_pointer_pos()
    {
        lut_ui.split = ((pointer.x - shown.left()) / shown.width()).clamp(0.0, 1.0);
    }
    let x = shown.left() + shown.width() * lut_ui.split;
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
    if let Some((_, button)) = toolbar(ui, rect, &[&sources]) {
        lut_ui.source = [LutSource::Chart, LutSource::Image][button];
        ui.ctx().request_repaint();
    }
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
    view_3d(ui, rect, preview);
    if let Some((_, button)) = toolbar(ui, rect, &[&shape_tools(preview.shape)]) {
        preview.shape = Shape::ALL[button];
        ui.ctx().request_repaint();
    }
}

/// Shows the 3D preview in `rect` and handles orbit input; see docs/features/canvas.md#3d-controls.
fn view_3d(ui: &mut Ui, rect: Rect, preview: &mut Preview3d) {
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
    } else if response.dragged_by(PointerButton::Primary)
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
    /// Three circles, filled for the shown channels.
    Channels(Channels),
}

struct Tool {
    glyph: Glyph,
    name: String,
    on: bool,
    enabled: bool,
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
