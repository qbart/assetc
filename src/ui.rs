use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{
    Align, Align2, FontId, Frame, Layout, Margin, PointerButton, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, UiBuilder, Vec2, pos2, vec2,
};

use std::collections::HashSet;
use std::path::PathBuf;
use std::thread::JoinHandle;

use crate::assets::Assets;
use crate::build::{self, Outcome};
use crate::config::Project;
use crate::icons::Icon;
use crate::inspect::{self, Inspection, Location};
use crate::logo;
use crate::manifest::Kind;
use crate::theme::{
    self, ACCENT, BG_BAR, BG_CENTER, BG_HEADER, BG_PANEL, CHECKER_DARK, CHECKER_LIGHT, STROKE,
    TEXT, TEXT_WEAK,
};
use crate::tree::{FileTrees, Listing, Node};

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
    /// Images shown tiled 3 × 3 in the canvas, by path.
    tiled: HashSet<PathBuf>,
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
    preview: Option<egui::TextureHandle>,
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
            tiled: HashSet::new(),
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
    Assets,
    Settings,
}

impl InspectorTab {
    const ALL: [Self; 3] = [Self::Info, Self::Assets, Self::Settings];

    fn icon(self) -> Icon {
        match self {
            Self::Info => Icon::Info,
            Self::Assets => Icon::Files,
            Self::Settings => Icon::Settings,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Assets => "Assets",
            Self::Settings => "Settings",
        }
    }

    fn enabled(self, selection: Option<&Selection>) -> bool {
        match self {
            Self::Info | Self::Settings => true,
            // Only compiled files are asset containers for now.
            Self::Assets => selection.is_some_and(|s| {
                s.location == Location::Output
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

fn shell(
    mut contexts: EguiContexts,
    mut state: ResMut<UiState>,
    project: Res<Project>,
    mut trees: ResMut<FileTrees>,
    mut assets: ResMut<Assets>,
    mut exit: MessageWriter<AppExit>,
    mut logo: Local<Option<egui::TextureHandle>>,
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
        .show(&mut root, |ui| status_bar(ui, &project, &state.build));

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

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_CENTER))
        .show(&mut root, |ui| {
            let state = &mut *state;
            canvas(
                ui,
                state.inspected.as_ref(),
                &mut state.view,
                &mut state.tiled,
            );
        });

    if std::mem::take(&mut state.start_build) {
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
        ctx.request_repaint();
    }

    Ok(())
}

fn inspect_selection(ctx: &egui::Context, project: &Project, selection: &Selection) -> Inspected {
    let mut inspection = inspect::inspect(project, selection.location, &selection.path);
    let max_side = ctx.input(|i| i.max_texture_side) as u32;
    let preview = inspection.preview.take().map(|image| {
        let image = inspect::fit_within(image, max_side);
        let size = [image.width as usize, image.height as usize];
        let image = egui::ColorImage::from_rgba_unmultiplied(size, &image.pixels);
        let options = egui::TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Linear,
            ..Default::default()
        };
        ctx.load_texture("canvas_preview", image, options)
    });
    Inspected {
        path: selection.path.clone(),
        inspection,
        preview,
    }
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
        ui.menu_button("File", |ui| {
            if menu_item(ui, "Quit", None).clicked() {
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
    let width = ui.available_width().max(180.0);
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

fn status_bar(ui: &mut Ui, project: &Project, build: &BuildStatus) {
    ui.horizontal(|ui| {
        match build {
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
        Listing::Loaded(nodes) if nodes.is_empty() => placeholder(ui, "Empty"),
        Listing::Loaded(nodes) => {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    if let Some(path) = tree(ui, nodes, 0, location, state.selection.as_ref()) {
                        select(state, location, path);
                    }
                });
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

/// Draws `nodes`; returns the file clicked this frame, if any.
fn tree(
    ui: &mut Ui,
    nodes: &[Node],
    depth: usize,
    location: Location,
    selection: Option<&Selection>,
) -> Option<PathBuf> {
    let mut clicked = None;
    for node in nodes {
        match &node.children {
            Some(children) => {
                let id = ui.make_persistent_id(&node.path);
                let open = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
                let (chevron, icon) = if open {
                    (Icon::ChevronDown, Icon::FolderOpen)
                } else {
                    (Icon::ChevronRight, Icon::Folder)
                };
                if tree_row(ui, depth, Some(chevron), icon, &node.name, false).clicked() {
                    ui.data_mut(|d| d.insert_temp(id, !open));
                }
                if open {
                    clicked = tree(ui, children, depth + 1, location, selection).or(clicked);
                }
            }
            None => {
                let selected =
                    selection.is_some_and(|s| s.location == location && s.path == node.path);
                if tree_row(ui, depth, None, Icon::File, &node.name, selected).clicked() {
                    clicked = Some(node.path.clone());
                }
            }
        }
    }
    clicked
}

fn tree_row(
    ui: &mut Ui,
    depth: usize,
    chevron: Option<Icon>,
    icon: Icon,
    name: &str,
    selected: bool,
) -> Response {
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
    ui.painter().text(
        pos2(x + 38.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        TEXT,
    );
    response
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
            match (state.inspector_tab, &state.inspected, &state.selection) {
                (_, None, _) | (_, _, None) => placeholder(ui, "Nothing selected"),
                (InspectorTab::Info, Some(inspected), _) => info(ui, &inspected.inspection),
                (InspectorTab::Assets, _, Some(selection)) => {
                    assets_tab(ui, assets, &selection.path)
                }
                (InspectorTab::Settings, ..) => placeholder(ui, "No settings for this file"),
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

/// Center pane: the selected file's image with zoom and pan, see docs/features/canvas.md.
fn canvas(
    ui: &mut Ui,
    inspected: Option<&Inspected>,
    view: &mut View,
    tiled: &mut HashSet<PathBuf>,
) {
    let rect = ui.max_rect();
    let message = |ui: &mut Ui, text: &str| {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(14.0),
            TEXT_WEAK,
        );
    };
    let Some(inspected) = inspected else {
        return message(ui, "Select a file to preview");
    };
    let inspection = &inspected.inspection;
    if inspection.missing {
        return message(ui, &format!("{} no longer exists", inspection.name));
    }
    let Some(texture) = &inspected.preview else {
        return message(ui, inspection.note.unwrap_or("No preview"));
    };

    let ppp = ui.ctx().pixels_per_point();
    let tiles = if tiled.contains(&inspected.path) {
        3.0
    } else {
        1.0
    };
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
    checkerboard(&painter, shown);
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
    let is_tiled = tiles > 1.0;
    if toolbar(ui, rect, &[(Icon::Grid3x3, "Tile", is_tiled)]) == Some(0) {
        if is_tiled {
            tiled.remove(&inspected.path);
        } else {
            tiled.insert(inspected.path.clone());
        }
        ui.ctx().request_repaint();
    }
    if zoom_bar(ui, rect, view, fit) {
        ui.ctx().request_repaint();
    }
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

/// Toggle buttons `(icon, name, on)` in the canvas' top-right corner; returns the clicked index.
fn toolbar(ui: &mut Ui, canvas: Rect, buttons: &[(Icon, &str, bool)]) -> Option<usize> {
    const BUTTON: Vec2 = vec2(36.0, 32.0);
    const RADIUS: u8 = 6;
    let width = BUTTON.x * buttons.len() as f32;
    let bar = Rect::from_min_size(
        pos2(canvas.right() - width - 8.0, canvas.top() + 8.0),
        vec2(width, BUTTON.y),
    );
    let painter = ui.painter_at(canvas);
    painter.rect_filled(bar, RADIUS, BG_BAR);

    let mut clicked = None;
    let last = buttons.len() - 1;
    for (i, &(icon, name, on)) in buttons.iter().enumerate() {
        let rect = Rect::from_min_size(bar.min + vec2(BUTTON.x * i as f32, 0.0), BUTTON);
        let response = ui
            .interact(rect, ui.id().with(("toolbar", i)), Sense::click_and_drag())
            .on_hover_text(name);
        // Round only the corners on the group's outer edge.
        let outer = |edge: bool| if edge { RADIUS } else { 0 };
        let radius = egui::CornerRadius {
            nw: outer(i == 0),
            sw: outer(i == 0),
            ne: outer(i == last),
            se: outer(i == last),
        };
        let tint = if on {
            painter.rect_filled(rect, radius, ACCENT);
            BG_BAR
        } else if response.hovered() {
            painter.rect_filled(rect, radius, BG_HEADER);
            TEXT
        } else {
            TEXT_WEAK
        };
        if i > 0 {
            painter.vline(rect.left(), rect.y_range(), Stroke::new(1.0, STROKE));
        }
        icon.image(18.0, tint)
            .paint_at(ui, Rect::from_center_size(rect.center(), vec2(18.0, 18.0)));
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
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
