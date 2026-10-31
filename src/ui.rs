use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{
    Align, Align2, FontId, Frame, Layout, Margin, Rect, Response, RichText, Sense, Stroke, Ui,
    UiBuilder, vec2,
};

use std::thread::JoinHandle;

use crate::build::{self, Outcome};
use crate::config::Project;
use crate::icons::Icon;
use crate::logo;
use crate::theme::{self, ACCENT, BG_BAR, BG_CENTER, BG_HEADER, BG_PANEL, STROKE, TEXT, TEXT_WEAK};
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
    Settings,
}

impl InspectorTab {
    const ALL: [Self; 2] = [Self::Info, Self::Settings];

    fn icon(self) -> Icon {
        match self {
            Self::Info => Icon::Info,
            Self::Settings => Icon::Settings,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Settings => "Settings",
        }
    }
}

fn scan_trees(mut commands: Commands, project: Res<Project>) {
    commands.insert_resource(FileTrees::scan(&project));
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
            .show(&mut root, |ui| right_sidebar(ui, &mut state));
    }

    if std::mem::take(&mut state.start_build) {
        start_build(&mut state, &project);
    }
    poll_build(&mut state);

    if std::mem::take(&mut state.rescan) {
        *trees = FileTrees::scan(&project);
    }

    Ok(())
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
    // Padded buttons with no gaps so the whole item, not just its text, is clickable.
    let style = |style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        style.spacing.button_padding = vec2(10.0, 6.0);
        style.spacing.item_spacing.x = 0.0;
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
            if ui.button("Quit").clicked() {
                exit.write(AppExit::Success);
            }
        });
        ui.add_enabled_ui(false, |ui| {
            ui.menu_button("Edit", |_| {});
        });
        ui.menu_button("View", |ui| {
            ui.checkbox(&mut state.show_left_sidebar, "Left Sidebar");
            ui.checkbox(&mut state.show_right_sidebar, "Right Sidebar");
        });
        ui.menu_button("Help", |ui| {
            ui.label(format!("assetc {}", env!("CARGO_PKG_VERSION")));
        });
    });
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
                    tree(ui, nodes, 0);
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

fn tree(ui: &mut Ui, nodes: &[Node], depth: usize) {
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
                if tree_row(ui, depth, Some(chevron), icon, &node.name).clicked() {
                    ui.data_mut(|d| d.insert_temp(id, !open));
                }
                if open {
                    tree(ui, children, depth + 1);
                }
            }
            None => {
                tree_row(ui, depth, None, Icon::File, &node.name);
            }
        }
    }
}

fn tree_row(ui: &mut Ui, depth: usize, chevron: Option<Icon>, icon: Icon, name: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, BG_HEADER);
    }
    let icon_rect =
        |x: f32| Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0));
    let x = rect.left() + 6.0 + depth as f32 * 14.0;
    if let Some(chevron) = chevron {
        chevron
            .image(14.0, TEXT_WEAK)
            .paint_at(ui, icon_rect(x).shrink(1.0));
    }
    icon.image(16.0, TEXT_WEAK)
        .paint_at(ui, icon_rect(x + 16.0));
    ui.painter().text(
        egui::pos2(x + 38.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        TEXT,
    );
    response
}

fn right_sidebar(ui: &mut Ui, state: &mut UiState) {
    let assets_height = (ui.available_height() * 0.4).round();

    egui::Panel::top("assets")
        .resizable(false)
        .exact_size(assets_height)
        .frame(Frame::new().fill(BG_PANEL))
        .show(ui, |ui| {
            section_header(ui, "Assets", |ui| {
                icon_button(ui, Icon::Plus, "Add asset", false);
            });
            placeholder(ui, "No assets");
        });

    section_header(ui, "Inspector", |_| {});

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
                let response = icon_tab(ui, tab.icon(), state.inspector_tab == tab);
                if response.on_hover_text(tab.name()).clicked() {
                    state.inspector_tab = tab;
                }
            }
        });

    egui::CentralPanel::default()
        .frame(Frame::new().fill(BG_PANEL))
        .show(ui, |ui| match state.inspector_tab {
            InspectorTab::Info | InspectorTab::Settings => placeholder(ui, "Nothing selected"),
        });
}

fn section_header(ui: &mut Ui, title: &str, actions: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(BG_HEADER)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
            });
        });
}

fn placeholder(ui: &mut Ui, text: &str) {
    Frame::new()
        .inner_margin(Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.weak(text);
        });
}
