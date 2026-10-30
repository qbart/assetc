use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{Frame, Margin, RichText, Stroke, Ui, vec2};

use crate::config::Project;
use crate::theme::{self, ACCENT, BG_BAR, BG_CENTER, BG_HEADER, BG_PANEL, STROKE};
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
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_left_sidebar: true,
            show_right_sidebar: true,
            inspector_tab: InspectorTab::Info,
            browser_tab: BrowserTab::Input,
            rescan: false,
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

    fn icon(self) -> &'static str {
        match self {
            Self::Info => "ℹ",
            Self::Settings => "⚙",
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
    mut themed: Local<bool>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    if !*themed {
        theme::apply(&ctx);
        *themed = true;
    }

    let mut root = Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );

    egui::Panel::top("menu_bar")
        .frame(bar_frame())
        .show(&mut root, |ui| menu_bar(ui, &mut state, &mut exit));

    egui::Panel::bottom("status_bar")
        .frame(bar_frame())
        .show(&mut root, |ui| status_bar(ui, &project));

    if state.show_left_sidebar {
        egui::Panel::left("left_sidebar")
            .resizable(true)
            .default_size(260.0)
            .min_size(200.0)
            .frame(Frame::new().fill(BG_PANEL))
            .show(&mut root, |ui| left_sidebar(ui, &mut state, &trees));
    }

    if state.show_right_sidebar {
        egui::Panel::right("right_sidebar")
            .resizable(true)
            .default_size(320.0)
            .min_size(220.0)
            .frame(Frame::new().fill(BG_PANEL))
            .show(&mut root, |ui| right_sidebar(ui, &mut state));
    }

    if std::mem::take(&mut state.rescan) {
        *trees = FileTrees::scan(&project);
    }

    Ok(())
}

fn bar_frame() -> Frame {
    Frame::new()
        .fill(BG_BAR)
        .inner_margin(Margin::symmetric(8, 4))
        .stroke(Stroke::new(1.0, STROKE))
}

fn menu_bar(ui: &mut Ui, state: &mut UiState, exit: &mut MessageWriter<AppExit>) {
    egui::MenuBar::new().ui(ui, |ui| {
        logo(ui);
        ui.add_space(6.0);
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

fn logo(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), egui::Sense::hover());
    let c = rect.center();
    let r = 6.0;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
            c + vec2(-r, 0.0),
        ],
        ACCENT,
        Stroke::NONE,
    ));
}

fn status_bar(ui: &mut Ui, project: &Project) {
    ui.horizontal(|ui| {
        if project.root.is_some() {
            ui.label("Ready");
        } else {
            ui.label("No assetc.toml found (run assetc init)");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("assetc {}", env!("CARGO_PKG_VERSION")));
        });
    });
}

fn left_sidebar(ui: &mut Ui, state: &mut UiState, trees: &FileTrees) {
    Frame::new()
        .fill(BG_HEADER)
        .inner_margin(Margin::symmetric(4, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                for tab in BrowserTab::ALL {
                    let button = egui::Button::new(tab.name()).selected(state.browser_tab == tab);
                    if ui.add(button).clicked() {
                        state.browser_tab = tab;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟳").on_hover_text("Rescan").clicked() {
                        state.rescan = true;
                    }
                });
            });
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
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                Frame::new()
                    .inner_margin(Margin::symmetric(8, 6))
                    .show(ui, |ui| tree(ui, nodes));
            });
        }
    }
}

fn tree(ui: &mut Ui, nodes: &[Node]) {
    for node in nodes {
        match &node.children {
            Some(children) => {
                egui::CollapsingHeader::new(format!("🗀 {}", node.name))
                    .id_salt(&node.path)
                    .default_open(false)
                    .show(ui, |ui| tree(ui, children));
            }
            None => {
                ui.horizontal(|ui| {
                    // Line files up with folder labels, past the collapse arrow.
                    ui.add_space(ui.spacing().indent);
                    ui.label(format!("🗋 {}", node.name));
                });
            }
        }
    }
}

fn right_sidebar(ui: &mut Ui, state: &mut UiState) {
    let assets_height = (ui.available_height() * 0.4).round();

    egui::Panel::top("assets")
        .resizable(false)
        .exact_size(assets_height)
        .frame(Frame::new().fill(BG_PANEL))
        .show(ui, |ui| {
            section_header(ui, "Assets", |ui| {
                ui.add_enabled(false, egui::Button::new("+").small());
            });
            placeholder(ui, "No assets");
        });

    section_header(ui, "Inspector", |_| {});

    egui::Panel::left("inspector_tabs")
        .exact_size(36.0)
        .resizable(false)
        .frame(Frame::new().fill(BG_BAR).inner_margin(Margin::same(4)))
        .show(ui, |ui| {
            for tab in InspectorTab::ALL {
                let button = egui::Button::new(RichText::new(tab.icon()).size(16.0))
                    .selected(state.inspector_tab == tab)
                    .min_size(vec2(28.0, 28.0));
                if ui.add(button).on_hover_text(tab.name()).clicked() {
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
