use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui::{Color32, Frame, Margin, RichText, Stroke, Ui, vec2};

// Palette, see docs/features/gui.md.
const BG_CENTER: Color32 = Color32::from_rgb(0x2B, 0x2B, 0x2D);
const BG_PANEL: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x22);
const BG_BAR: Color32 = Color32::from_rgb(0x18, 0x18, 0x1A);
const BG_HEADER: Color32 = Color32::from_rgb(0x25, 0x25, 0x28);
const STROKE: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x32);
const TEXT: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD8);
const TEXT_WEAK: Color32 = Color32::from_rgb(0x7A, 0x7A, 0x82);
const ACCENT: Color32 = Color32::from_rgb(0x6E, 0x56, 0xCF);

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        let [r, g, b, _] = BG_CENTER.to_array();
        app.insert_resource(ClearColor(Color::srgb_u8(r, g, b)))
            .init_resource::<UiState>()
            .add_systems(EguiPrimaryContextPass, shell);
    }
}

#[derive(Resource)]
struct UiState {
    show_sidebar: bool,
    inspector_tab: InspectorTab,
    status: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_sidebar: true,
            inspector_tab: InspectorTab::Info,
            status: "Ready".into(),
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

fn shell(
    mut contexts: EguiContexts,
    mut state: ResMut<UiState>,
    mut exit: MessageWriter<AppExit>,
    mut themed: Local<bool>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    if !*themed {
        apply_theme(&ctx);
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
        .show(&mut root, |ui| status_bar(ui, &state));

    if state.show_sidebar {
        egui::Panel::right("sidebar")
            .resizable(true)
            .default_size(320.0)
            .min_size(220.0)
            .frame(Frame::new().fill(BG_PANEL))
            .show(&mut root, |ui| sidebar(ui, &mut state));
    }

    Ok(())
}

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.window_stroke = Stroke::new(1.0, STROKE);
    v.extreme_bg_color = BG_BAR;
    v.faint_bg_color = BG_HEADER;
    v.weak_text_color = Some(TEXT_WEAK);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = BG_PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, STROKE);
    w.noninteractive.fg_stroke.color = TEXT;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_fill = BG_HEADER;
    w.inactive.fg_stroke.color = TEXT;
    w.hovered.weak_bg_fill = STROKE;
    w.hovered.bg_fill = STROKE;
    w.hovered.bg_stroke = Stroke::NONE;
    w.active.weak_bg_fill = ACCENT;
    w.active.bg_fill = ACCENT;
    w.open.weak_bg_fill = STROKE;

    ctx.set_visuals(v);
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
            ui.checkbox(&mut state.show_sidebar, "Sidebar");
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

fn status_bar(ui: &mut Ui, state: &UiState) {
    ui.horizontal(|ui| {
        ui.label(&state.status);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("assetc {}", env!("CARGO_PKG_VERSION")));
        });
    });
}

fn sidebar(ui: &mut Ui, state: &mut UiState) {
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
