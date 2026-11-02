use bevy_egui::egui::{self, Color32, Stroke};

// Palette, see docs/features/gui.md.
pub const BG_CENTER: Color32 = Color32::from_rgb(0x2B, 0x2B, 0x2D);
pub const BG_PANEL: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x22);
pub const BG_BAR: Color32 = Color32::from_rgb(0x18, 0x18, 0x1A);
pub const BG_HEADER: Color32 = Color32::from_rgb(0x25, 0x25, 0x28);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x38, 0x38, 0x3D);
pub const STROKE: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x32);
pub const TEXT: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD8);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(0x7A, 0x7A, 0x82);
pub const ACCENT: Color32 = Color32::from_rgb(0xF0, 0x88, 0x3E);
pub const CHECKER_LIGHT: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3E);
pub const CHECKER_DARK: Color32 = Color32::from_rgb(0x2E, 0x2E, 0x32);
pub const CHANNEL_R: Color32 = Color32::from_rgb(0xE5, 0x48, 0x4D);
pub const CHANNEL_G: Color32 = Color32::from_rgb(0x46, 0xA7, 0x58);
pub const CHANNEL_B: Color32 = Color32::from_rgb(0x3E, 0x8E, 0xED);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.window_stroke = Stroke::new(1.0, STROKE);
    v.extreme_bg_color = BG_BAR;
    v.faint_bg_color = BG_HEADER;
    v.weak_text_color = Some(TEXT_WEAK);
    v.selection.bg_fill = ACCENT;
    // Dark text on the accent fill; see docs/features/gui.md#look.
    v.selection.stroke = Stroke::new(1.0, BG_BAR);

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
