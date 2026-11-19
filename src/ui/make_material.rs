//! The Make Material dialog: a PBR material from selected textures; see
//! docs/features/groups.md#make-material.

use std::path::Path;

use bevy_egui::egui::{self, Align, ComboBox, Frame, Layout, RichText, Stroke, TextEdit, vec2};

use super::config_dialog::dialog_button;
use crate::groups::MaterialPlan;
use crate::material::{NormalY, Slot};
use crate::theme::{BG_PANEL, CHANNEL_R, STROKE, TEXT, TEXT_WEAK};

/// How the dialog ends this frame.
#[derive(Debug, PartialEq, Eq)]
pub enum End {
    Open,
    Cancel,
    Make,
}

const WIDTH: f32 = 560.0;

pub fn show(ctx: &egui::Context, plan: &mut MaterialPlan) -> End {
    let frame = Frame::window(&ctx.global_style())
        .fill(BG_PANEL)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(8)
        .inner_margin(24);
    let mut end = End::Open;
    let modal = egui::Modal::new(egui::Id::new("make_material"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.label(RichText::new("Make Material").size(16.0).color(TEXT));
            ui.add_space(12.0);
            fields(ui, plan);
            let problems = plan.problems();
            if !problems.is_empty() {
                ui.add_space(8.0);
                for problem in &problems {
                    ui.label(RichText::new(problem).size(13.0).color(CHANNEL_R));
                }
            }
            ui.add_space(24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if dialog_button(ui, "Make Material", true, problems.is_empty()).clicked() {
                    end = End::Make;
                }
                if dialog_button(ui, "Cancel", false, true).clicked() {
                    end = End::Cancel;
                }
            });
        });
    if end == End::Open && modal.should_close() {
        end = End::Cancel;
    }
    end
}

fn label(text: &str) -> RichText {
    RichText::new(text).size(13.0).color(TEXT)
}

fn weak(text: &str) -> RichText {
    RichText::new(text).size(13.0).color(TEXT_WEAK)
}

fn slot_name(slot: Option<Slot>) -> &'static str {
    slot.map_or("Not used", Slot::label)
}

fn normal_name(y: NormalY) -> &'static str {
    match y {
        NormalY::Gl => "OpenGL (+Y up)",
        NormalY::Dx => "DirectX (+Y down)",
    }
}

/// Where a file goes, as the dialog says it: relative to the files' folder, or `stays`.
fn goes_to(plan: &MaterialPlan, file: &Path, slot: Option<Slot>) -> String {
    let Some(slot) = slot else {
        return "stays".into();
    };
    let to = plan.destination(file, slot);
    if to == file {
        return "stays".into();
    }
    format!("→ {}", crate::adata::relative(&plan.source_dir(), &to))
}

fn fields(ui: &mut egui::Ui, plan: &mut MaterialPlan) {
    ui.horizontal(|ui| {
        ui.add_sized([96.0, 22.0], egui::Label::new(label("Name")));
        ui.add(
            TextEdit::singleline(&mut plan.name)
                .desired_width(WIDTH - 104.0)
                .font(egui::FontId::proportional(13.0)),
        );
    });
    ui.add_space(12.0);
    egui::ScrollArea::vertical()
        .max_height(280.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::Grid::new("make_material_files")
                .num_columns(3)
                .spacing(vec2(12.0, 6.0))
                .show(ui, |ui| {
                    for i in 0..plan.files.len() {
                        let (file, slot) = plan.files[i].clone();
                        let name = file
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.add_sized([200.0, 20.0], egui::Label::new(label(&name)).truncate())
                            .on_hover_text(&name);
                        let mut chosen = slot;
                        ComboBox::from_id_salt(("make_material_slot", i))
                            .width(120.0)
                            .selected_text(slot_name(chosen))
                            .show_ui(ui, |ui| {
                                for option in Slot::ALL.map(Some).into_iter().chain([None]) {
                                    ui.selectable_value(&mut chosen, option, slot_name(option));
                                }
                            });
                        plan.files[i].1 = chosen;
                        ui.add(egui::Label::new(weak(&goes_to(plan, &file, chosen))).truncate());
                        ui.end_row();
                    }
                });
        });
    ui.add_space(12.0);
    if plan.used().any(|(_, s)| s == Slot::Normal) {
        ui.horizontal(|ui| {
            ui.add_sized([96.0, 22.0], egui::Label::new(label("Normal map")));
            ComboBox::from_id_salt("make_material_normal")
                .width(160.0)
                .selected_text(normal_name(plan.normal_y))
                .show_ui(ui, |ui| {
                    for y in [NormalY::Gl, NormalY::Dx] {
                        ui.selectable_value(&mut plan.normal_y, y, normal_name(y));
                    }
                });
        });
        ui.add_space(6.0);
    }
    ui.checkbox(&mut plan.folder, label("Put the files in a new folder"));
    ui.checkbox(
        &mut plan.rename,
        label("Rename files to the naming convention"),
    );
    if plan.files.iter().any(|(_, s)| s.is_none()) {
        ui.add_space(6.0);
        ui.label(weak("Files not used stay where they are."));
    }
}
