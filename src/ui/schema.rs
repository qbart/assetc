//! The `.aschema` editor: the Inspector's Types tab and the canvas; see
//! docs/features/schemas.md.

use std::cell::Cell;

use serde_json::{Number, Value};

use super::curve::{self, CurveSettings, CurveState, curve_preview};
use super::*;
use crate::aschema::{Field, Group, Problem, Ty, TypeDef, parse_type};
use crate::theme::CHANNEL_R;

/// A schema being edited.
#[derive(Default)]
pub(super) struct SchemaEdit {
    pub(super) edit: Schema,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    pub(super) saved: Schema,
    /// Unfolded field rows, by type and field name.
    open: HashSet<(usize, String)>,
    /// Curve and gradient editors of defaults, by type and field name.
    curves: HashMap<(usize, String), CurveState>,
    gradients: HashMap<(usize, String), GradientCard>,
    /// The color picker of a colour default.
    picker: Picker,
    curve_settings: CurveSettings,
    /// Whether a group was being dragged last frame: groups then show their headers only.
    group_drag: bool,
}

impl SchemaEdit {
    /// `schema` as read, keeping `previous`'s editor state.
    pub(super) fn new(schema: Schema, previous: SchemaEdit) -> Self {
        Self {
            edit: schema.clone(),
            saved: schema,
            ..previous
        }
    }
}

/// The schema to write once nothing is being dragged or typed into, as for gradients.
fn schema_commit(ui: &Ui, edit: &mut SchemaEdit) -> Option<Schema> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

/// A text field whose edits apply when it loses focus: the new text then, when it changed.
pub(super) fn commit_field(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    width: f32,
    hint: &str,
) -> Option<String> {
    let id = ui.id().with(salt);
    let mut text = ui
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| current.to_owned());
    let field = egui::TextEdit::singleline(&mut text)
        .id(id.with("field"))
        .font(egui::TextStyle::Monospace)
        .hint_text(hint);
    let response = ui.add_sized(vec2(width, 22.0), field);
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, text));
        None
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        (response.lost_focus() && text != current).then_some(text)
    }
}

/// A number field applied on focus loss: `Some(None)` when emptied.
fn number_field(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: Option<&Number>,
    width: f32,
) -> Option<Option<Number>> {
    let text = current.map(Number::to_string).unwrap_or_default();
    let new = commit_field(ui, salt, &text, width, "")?;
    let new = new.trim();
    if new.is_empty() {
        return Some(None);
    }
    if let Ok(i) = new.parse::<i64>() {
        return Some(Some(i.into()));
    }
    new.parse::<f64>().ok().and_then(Number::from_f64).map(Some)
}

/// A free name: `base`, `base2`, … not in `taken`.
fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                base.to_owned()
            } else {
                format!("{base}{n}")
            }
        })
        .find(|n| !taken(n))
        .unwrap_or_default()
}

fn size_text(t: &TypeDef) -> String {
    let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    match &t.body {
        Body::Struct { fields, .. } => plural(fields.len(), "field"),
        Body::Enum { values } => plural(values.len(), "value"),
    }
}

/// The Types tab: the schema's types, to choose, add, reorder and delete; see
/// docs/features/schemas.md#types-tab.
pub(super) fn types_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    chosen: &mut usize,
) -> Option<Schema> {
    let edit = match &mut inspected.schema {
        Some(Ok(edit)) => edit,
        Some(Err((e, _))) => {
            placeholder(ui, e);
            return None;
        }
        None => {
            placeholder(ui, "No types");
            return None;
        }
    };
    let problems = edit.edit.check();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                ui.horizontal(|ui| {
                    let types = &mut edit.edit.types;
                    let taken = |types: &[TypeDef], n: &str| types.iter().any(|t| t.name == n);
                    let add =
                        egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add struct");
                    if ui.add(add).clicked() {
                        let name = free_name("Struct", |n| taken(types, n));
                        types.push(TypeDef::new_struct(name));
                        *chosen = types.len() - 1;
                    }
                    let add =
                        egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add enum");
                    if ui.add(add).clicked() {
                        let name = free_name("Enum", |n| taken(types, n));
                        types.push(TypeDef::new_enum(name));
                        *chosen = types.len() - 1;
                    }
                });
                ui.add_space(8.0);
                if edit.edit.types.is_empty() {
                    ui.weak("No types");
                }
                let dragging = Cell::new(None);
                let dropped = Cell::new(None);
                let mut delete = None;
                let mut rects = Vec::new();
                for (i, t) in edit.edit.types.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                    rects.push(rect);
                    let fill = if *chosen == i {
                        ACCENT.gamma_multiply(0.25)
                    } else if response.hovered() {
                        BG_HEADER
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(rect, 4, fill);
                    let grip = ui
                        .scope_builder(
                            UiBuilder::new().max_rect(Rect::from_min_size(
                                rect.min + vec2(2.0, 2.0),
                                vec2(24.0, 24.0),
                            )),
                            drag_handle,
                        )
                        .inner;
                    if grip.dragged() {
                        dragging.set(Some(i));
                    }
                    if grip.drag_stopped() {
                        dropped.set(Some(i));
                    }
                    let painter = ui.painter();
                    painter.text(
                        pos2(rect.left() + 30.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        t.kind(),
                        FontId::proportional(11.0),
                        TEXT_WEAK,
                    );
                    painter.text(
                        pos2(rect.left() + 74.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        &t.name,
                        FontId::proportional(14.0),
                        TEXT,
                    );
                    let right = if *chosen == i { 34.0 } else { 8.0 };
                    let size = painter.text(
                        pos2(rect.right() - right, rect.center().y),
                        Align2::RIGHT_CENTER,
                        size_text(t),
                        FontId::proportional(13.0),
                        TEXT_WEAK,
                    );
                    let messages: Vec<&str> = problems
                        .iter()
                        .filter(|p| p.type_index == i)
                        .map(|p| p.message.as_str())
                        .collect();
                    if !messages.is_empty() {
                        let at = Rect::from_center_size(
                            pos2(size.left() - 14.0, rect.center().y),
                            vec2(16.0, 16.0),
                        );
                        problem_icon(ui, at, ("type_problems", i), &messages);
                    }
                    if *chosen == i {
                        let bin =
                            Rect::from_min_max(pos2(rect.right() - 30.0, rect.top()), rect.max);
                        let clicked = ui
                            .scope_builder(
                                UiBuilder::new()
                                    .max_rect(bin)
                                    .layout(Layout::right_to_left(Align::Center)),
                                |ui| icon_button(ui, Icon::Trash2, "Delete", true).clicked(),
                            )
                            .inner;
                        if clicked {
                            delete = Some(i);
                        }
                    }
                    if response.clicked() {
                        *chosen = i;
                    }
                }
                if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
                    let t = edit.edit.types.remove(from);
                    edit.edit.types.insert(to, t);
                    if *chosen == from {
                        *chosen = to;
                    }
                    edit.open.clear();
                }
                if let Some(i) = delete {
                    edit.edit.types.remove(i);
                    *chosen = chosen.saturating_sub(usize::from(*chosen >= i && *chosen > 0));
                    edit.open.clear();
                }
            });
        });
    schema_commit(ui, edit)
}

/// The canvas editor of the chosen type; see docs/features/schemas.md#canvas.
pub(super) fn canvas_schema(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    chosen: &mut HashMap<PathBuf, usize>,
) -> Option<Schema> {
    let chosen = chosen.entry(inspected.path.clone()).or_default();
    let mut ui = ui.new_child(
        UiBuilder::new()
            .id_salt("schema_editor")
            .max_rect(rect.shrink(16.0))
            .layout(Layout::top_down(Align::Min)),
    );
    let ui = &mut ui;
    let edit = match &mut inspected.schema {
        Some(Ok(edit)) => edit,
        Some(Err((e, text))) => {
            ui.label(RichText::new(e.as_str()).color(CHANNEL_R));
            ui.add_space(8.0);
            egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                ui.label(RichText::new(text.as_str()).monospace().color(TEXT_WEAK));
            });
            return None;
        }
        None => return None,
    };
    if edit.edit.types.is_empty() {
        ui.weak("No types: add one in the Inspector's Types tab");
        return schema_commit(ui, edit);
    }
    *chosen = (*chosen).min(edit.edit.types.len() - 1);
    let problems = edit.edit.check();
    let snapshot = edit.edit.clone();
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        property_style(ui);
        let ti = *chosen;
        type_header(ui, edit, ti, &problems);
        ui.add_space(12.0);
        match edit.edit.types[ti].body {
            Body::Struct { .. } => struct_editor(ui, edit, &snapshot, ti, &problems),
            Body::Enum { .. } => enum_editor(ui, edit, ti),
        }
    });
    schema_commit(ui, edit)
}

/// The problem icon over `rect`, listing `messages` on hover; see
/// docs/features/schemas.md#problems.
fn problem_icon(
    ui: &Ui,
    rect: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    messages: &[&str],
) {
    Icon::Bug.image(16.0, CHANNEL_R).paint_at(ui, rect);
    ui.interact(rect, ui.id().with(salt), Sense::hover())
        .on_hover_text(messages.join("\n"));
}

/// The problem icon as a widget, when there are `messages`.
fn problem_widget(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, messages: &[&str]) {
    if messages.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
    problem_icon(
        ui,
        Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
        salt,
        messages,
    );
}

/// Kind, name and doc of type `ti`, and the problems of the type itself.
fn type_header(ui: &mut Ui, edit: &mut SchemaEdit, ti: usize, problems: &[Problem]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let t = &edit.edit.types[ti];
        ui.label(RichText::new(t.kind()).color(TEXT_WEAK));
        let old = t.name.clone();
        if let Some(new) = commit_field(ui, ("type_name", ti), &old, 180.0, "Name") {
            let schema = &edit.edit;
            let ok = aschema::is_identifier(&new)
                && !aschema::is_reserved(&new)
                && !schema.types.iter().any(|t| t.name == new);
            if ok {
                edit.edit.types[ti].name = new.clone();
                edit.edit.rename_type(&old, &new);
            }
        }
        let messages: Vec<&str> = problems
            .iter()
            .filter(|p| p.type_index == ti && p.field.is_none())
            .map(|p| p.message.as_str())
            .collect();
        let doc = &mut edit.edit.types[ti].doc;
        let field = egui::TextEdit::singleline(doc).hint_text("Doc");
        let room = ui.available_width() - if messages.is_empty() { 0.0 } else { 32.0 };
        ui.add_sized(vec2(room.max(120.0), 22.0), field);
        problem_widget(ui, "type_header_problems", &messages);
    });
}

/// Widths in a field's row: name, type, and the default's editor.
const NAME_W: f32 = 140.0;
const TYPE_W: f32 = 150.0;
const DEFAULT_W: f32 = 200.0;

/// The struct's groups as shown: its `groups` in order, then names only fields use.
fn group_names(fields: &[Field], groups: &[Group]) -> Vec<String> {
    let mut names: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
    for f in fields {
        if !f.group.is_empty() && !names.contains(&f.group) {
            names.push(f.group.clone());
        }
    }
    names
}

/// Puts the fields in the order shown (ungrouped, then group by group) and gives every group of
/// `order` an entry, in that order; see docs/features/schemas.md#struct.
fn arrange(fields: &mut [Field], groups: &mut Vec<Group>, order: &[String]) {
    *groups = order
        .iter()
        .map(|name| {
            groups
                .iter()
                .find(|g| g.name == *name)
                .cloned()
                .unwrap_or_else(|| Group {
                    name: name.clone(),
                    ..Group::default()
                })
        })
        .collect();
    fields.sort_by_key(|f| {
        if f.group.is_empty() {
            0
        } else {
            order
                .iter()
                .position(|g| *g == f.group)
                .map_or(usize::MAX, |i| i + 1)
        }
    });
}

/// The struct's sections: Ungrouped, the group cards and Add group; see
/// docs/features/schemas.md#struct.
fn struct_editor(
    ui: &mut Ui,
    edit: &mut SchemaEdit,
    snapshot: &Schema,
    ti: usize,
    problems: &[Problem],
) {
    let Body::Struct { fields, groups } = &edit.edit.types[ti].body else {
        return;
    };
    let names = group_names(fields, groups);
    let folded = edit.group_drag;
    ui.label(RichText::new("Ungrouped").size(14.0).color(TEXT_WEAK));
    ui.add_space(4.0);
    if !folded {
        ui.push_id("ungrouped", |ui| {
            field_rows(ui, edit, snapshot, ti, "", &names, problems);
        });
    }
    ui.add_space(12.0);

    let dragging = Cell::new(None);
    let dropped = Cell::new(None);
    let mut rects = Vec::new();
    let mut remove = None;
    let mut rename = None;
    let mut declare = false;
    for (gi, name) in names.iter().enumerate() {
        // Ids from the group's name, not the place: they stay while fields fold.
        let card = ui
            .push_id(("group", name), |ui| {
                Frame::new()
                    .fill(BG_PANEL)
                    .corner_radius(6)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let id = egui::Id::new(("schema_group_grip", ti, name));
                            let grip = drag_handle_with_id(ui, id);
                            if grip.dragged() {
                                dragging.set(Some(gi));
                            }
                            if grip.drag_stopped() {
                                dropped.set(Some(gi));
                            }
                            if let Some(new) = commit_field(ui, "name", name, NAME_W, "Group")
                                && !new.is_empty()
                                && !names.contains(&new)
                            {
                                rename = Some((name.clone(), new));
                            }
                            let Body::Struct { fields, groups } = &mut edit.edit.types[ti].body
                            else {
                                return;
                            };
                            let count = fields.iter().filter(|f| f.group == *name).count();
                            let entry = groups.iter_mut().find(|g| g.name == *name);
                            let mut group = entry.as_deref().cloned().unwrap_or_else(|| Group {
                                name: name.clone(),
                                ..Group::default()
                            });
                            ui.checkbox(&mut group.collapsed, "Collapsed");
                            let doc = egui::TextEdit::singleline(&mut group.doc).hint_text("Doc");
                            ui.add_sized(vec2((ui.available_width() - 110.0).max(80.0), 22.0), doc);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if icon_button(ui, Icon::X, "Remove group", true).clicked() {
                                    remove = Some(name.clone());
                                }
                                let plural = if count == 1 { "" } else { "s" };
                                ui.label(
                                    RichText::new(format!("{count} field{plural}"))
                                        .color(TEXT_WEAK),
                                );
                            });
                            match entry {
                                Some(entry) => *entry = group,
                                None if group.collapsed || !group.doc.is_empty() => {
                                    groups.push(group);
                                    declare = true;
                                }
                                None => {}
                            }
                        });
                        if !folded {
                            ui.add_space(4.0);
                            field_rows(ui, edit, snapshot, ti, name, &names, problems);
                        }
                    })
                    .response
                    .rect
            })
            .inner;
        rects.push(card);
        ui.add_space(8.0);
    }
    edit.group_drag = dragging.get().is_some();
    let Body::Struct { fields, groups } = &mut edit.edit.types[ti].body else {
        return;
    };
    let mut order = names.clone();
    let mut changed = declare;
    if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
        let name = order.remove(from);
        order.insert(to, name);
        changed = true;
    }
    if let Some((old, new)) = rename {
        for f in fields.iter_mut().filter(|f| f.group == old) {
            f.group = new.clone();
        }
        for g in groups.iter_mut().filter(|g| g.name == old) {
            g.name = new.clone();
        }
        for n in order.iter_mut().filter(|n| **n == old) {
            *n = new.clone();
        }
        changed = true;
    }
    if let Some(name) = remove {
        for f in fields.iter_mut().filter(|f| f.group == name) {
            f.group.clear();
        }
        groups.retain(|g| g.name != name);
        order.retain(|n| *n != name);
        changed = true;
    }
    if changed {
        arrange(fields, groups, &order);
    }
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add group");
    if ui.add(add).clicked() {
        let name = free_name("Group", |n| order.iter().any(|g| g == n));
        order.push(name);
        arrange(fields, groups, &order);
    }
}

/// The fields of group `group` (`""`: ungrouped) and its Add field; see
/// docs/features/schemas.md#struct.
fn field_rows(
    ui: &mut Ui,
    edit: &mut SchemaEdit,
    snapshot: &Schema,
    ti: usize,
    group: &str,
    names: &[String],
    problems: &[Problem],
) {
    let Body::Struct { fields, .. } = &edit.edit.types[ti].body else {
        return;
    };
    let shown: Vec<usize> = (0..fields.len())
        .filter(|&fi| fields[fi].group == group)
        .collect();
    let dragging = Cell::new(None);
    let dropped = Cell::new(None);
    let mut delete = None;
    let mut regroup = false;
    let mut rects = Vec::new();
    for (row, &fi) in shown.iter().enumerate() {
        let Body::Struct { fields, .. } = &edit.edit.types[ti].body else {
            return;
        };
        let name = fields[fi].name.clone();
        let taken: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
        let key = (ti, name.clone());
        let open = edit.open.contains(&key);
        let ty = parse_type(&fields[fi].ty).ok();
        let messages: Vec<&str> = problems
            .iter()
            .filter(|p| p.type_index == ti && p.field == Some(fi))
            .map(|p| p.message.as_str())
            .collect();
        let mut toggle = false;
        let rect = ui
            .push_id(("field", fi), |ui| {
                let background = ui.painter().add(egui::Shape::Noop);
                let rect = ui
                    .horizontal(|ui| {
                        ui.set_min_height(28.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let grip = drag_handle(ui);
                        if grip.dragged() {
                            dragging.set(Some(row));
                        }
                        if grip.drag_stopped() {
                            dropped.set(Some(row));
                        }
                        toggle |= icon_toggle(ui, Icon::Settings, "Details", open).clicked();
                        let Body::Struct { fields, .. } = &mut edit.edit.types[ti].body else {
                            return;
                        };
                        let field = &mut fields[fi];
                        if let Some(new) = commit_field(ui, "name", &field.name, NAME_W, "name")
                            && aschema::is_identifier(&new)
                            && !taken.contains(&new)
                        {
                            field.name = new;
                        }
                        type_cell(ui, field, snapshot, ty.is_some());
                        ui.checkbox(&mut field.optional, "Optional");
                        ui.add_space(6.0);
                        let mut editors = ValueEditors {
                            picker: &mut edit.picker,
                            curves: &mut edit.curves,
                            curve_settings: &mut edit.curve_settings,
                            gradients: &mut edit.gradients,
                            ti,
                        };
                        default_cell(ui, &mut editors, field, snapshot, ty.as_ref());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, Icon::X, "Remove field", true).clicked() {
                                delete = Some(fi);
                            }
                            problem_widget(ui, "problems", &messages);
                        });
                    })
                    .response
                    .rect;
                if ui.rect_contains_pointer(rect) {
                    ui.painter().set(
                        background,
                        egui::Shape::rect_filled(rect.expand(2.0), 4, BG_HEADER),
                    );
                }
                // A struct default's fields, under the row.
                if let Some((si, _)) = struct_of(snapshot, ty.as_ref())
                    && let Body::Struct { fields, .. } = &mut edit.edit.types[ti].body
                    && let Some(default) = fields[fi].default.as_mut().filter(|d| d.is_object())
                {
                    let mut editors = ValueEditors {
                        picker: &mut edit.picker,
                        curves: &mut edit.curves,
                        curve_settings: &mut edit.curve_settings,
                        gradients: &mut edit.gradients,
                        ti,
                    };
                    ui.indent("default_fields", |ui| {
                        let mut shown = vec![ti];
                        struct_value(ui, &mut editors, default, snapshot, si, &name, &mut shown);
                    });
                }
                if open {
                    ui.indent("details", |ui| {
                        regroup |= field_details(ui, edit, ti, fi, ty.as_ref(), snapshot, names);
                    });
                }
                rect
            })
            .inner;
        rects.push(rect);
        if toggle {
            if open {
                edit.open.remove(&key);
            } else {
                edit.open.insert(key);
            }
        }
    }
    let Body::Struct { fields, groups } = &mut edit.edit.types[ti].body else {
        return;
    };
    if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
        // Within the group: take it out, then put it before the field now at `to`.
        let f = fields.remove(shown[from]);
        let rest: Vec<usize> = (0..fields.len())
            .filter(|&k| fields[k].group == group)
            .collect();
        let at = rest
            .get(to)
            .copied()
            .unwrap_or_else(|| rest.last().map_or(shown[from], |&k| k + 1));
        fields.insert(at, f);
        regroup = true;
    }
    if let Some(fi) = delete {
        fields.remove(fi);
    }
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add field");
    if ui.add(add).clicked() {
        let name = free_name("field", |n| fields.iter().any(|f| f.name == n));
        fields.push(Field {
            group: group.to_owned(),
            ..Field::new(name, "string")
        });
        regroup = true;
    }
    if regroup {
        arrange(fields, groups, names);
    }
}

/// The type expression, red while it does not check, with a menu of types.
fn type_cell(ui: &mut Ui, field: &mut Field, schema: &Schema, parses: bool) {
    let valid = parses
        && parse_type(&field.ty)
            .ok()
            .is_some_and(|ty| check_known(schema, &ty));
    let color = if valid { TEXT } else { CHANNEL_R };
    let edit = egui::TextEdit::singleline(&mut field.ty)
        .font(egui::TextStyle::Monospace)
        .text_color(color);
    ui.add_sized(vec2(TYPE_W, 22.0), edit);
    let button = egui::Button::image(Icon::ChevronDown.image(14.0, TEXT_WEAK))
        .frame_when_inactive(false)
        .min_size(vec2(24.0, 22.0));
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        let mut picked = None;
        type_menu(ui, schema, &field.ty, false, &mut picked);
        if let Some(ty) = picked {
            field.ty = ty;
            ui.close();
        }
    });
}

/// The type menu's submenus, by kind; inside List (`in_list`) its items are wrapped in
/// `list<…>`. See docs/features/schemas.md#type-menu.
fn type_menu(
    ui: &mut Ui,
    schema: &Schema,
    current: &str,
    in_list: bool,
    picked: &mut Option<String>,
) {
    // Same size, color and text inset as `menu_row`.
    ui.spacing_mut().button_padding = vec2(24.0, 7.0);
    let named = |enums: bool| -> Vec<String> {
        schema
            .types
            .iter()
            .filter(|t| matches!(t.body, Body::Enum { .. }) == enums)
            .map(|t| t.name.clone())
            .collect()
    };
    let value: Vec<String> = aschema::PRIMITIVES
        .into_iter()
        .filter(|p| !matches!(*p, "rgb" | "rgba" | "curve"))
        .map(str::to_owned)
        .collect();
    let groups = [
        ("Value", value),
        (
            "Color",
            ["rgb", "rgba", aschema::GRADIENT]
                .map(str::to_owned)
                .to_vec(),
        ),
        ("Curve", vec!["curve".to_owned()]),
        ("Struct", named(false)),
        ("Enum", named(true)),
    ];
    for (title, items) in groups {
        ui.add_enabled_ui(!items.is_empty(), |ui| {
            ui.menu_button(RichText::new(title).size(14.0).color(TEXT), |ui| {
                for item in items {
                    let ty = if in_list {
                        format!("list<{item}>")
                    } else {
                        item.clone()
                    };
                    if menu_item(ui, &item, Some(ty == current)).clicked() {
                        *picked = Some(ty);
                    }
                }
            });
        });
    }
    ui.add_enabled_ui(false, |ui| {
        ui.menu_button(RichText::new("Reference").size(14.0).color(TEXT), |_| {});
    });
    if !in_list {
        ui.menu_button(RichText::new("List").size(14.0).color(TEXT), |ui| {
            type_menu(ui, schema, current, true, picked);
        });
    }
}

/// Whether every name in `ty` is a primitive or a type of the schema.
fn check_known(schema: &Schema, ty: &Ty) -> bool {
    match ty {
        Ty::Prim(_) => true,
        Ty::Named(n) => schema.type_index(n).is_some(),
        Ty::List(t) | Ty::Ref(t) => check_known(schema, t),
    }
}

/// The editors a default may open: shared, or kept per type and field path (curves and
/// gradients, `name` or `name/inner/…` for a nested struct's fields).
struct ValueEditors<'a> {
    picker: &'a mut Picker,
    curves: &'a mut HashMap<(usize, String), CurveState>,
    curve_settings: &'a mut CurveSettings,
    gradients: &'a mut HashMap<(usize, String), GradientCard>,
    ti: usize,
}

/// A popover under `response`, opened by double-clicking it, closed by clicking outside.
fn popover(response: &Response, width: f32, content: impl FnOnce(&mut Ui)) {
    let open = response
        .double_clicked()
        .then_some(egui::SetOpenCommand::Bool(true));
    egui::Popup::from_response(response)
        .open_memory(open)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(width)
        .show(|ui| {
            ui.set_width(width);
            content(ui);
        });
}

/// A field's Default: the checkbox, then, ticked, the default's editor. See
/// docs/features/schemas.md#defaults.
fn default_cell(
    ui: &mut Ui,
    editors: &mut ValueEditors,
    field: &mut Field,
    schema: &Schema,
    ty: Option<&Ty>,
) {
    let mut has = field.default.is_some();
    if ui.checkbox(&mut has, "Default").changed() {
        field.default =
            has.then(|| ty.map_or(Value::Null, |ty| aschema::implicit_value(schema, ty)));
    }
    if let Some(default) = &mut field.default {
        value_editor(ui, editors, default, schema, ty, &field.name);
    }
}

/// The struct a type names, if it is one.
fn struct_of<'s>(schema: &'s Schema, ty: Option<&Ty>) -> Option<(usize, &'s [Field])> {
    let Some(Ty::Named(n)) = ty else {
        return None;
    };
    let si = schema.type_index(n)?;
    match &schema.types[si].body {
        Body::Struct { fields, .. } => Some((si, fields)),
        Body::Enum { .. } => None,
    }
}

/// A value's editor in its type's data encoding; `path` keys its curve and gradient
/// editors. A struct shows how many of its fields are set; its rows are
/// [`struct_value`]'s.
fn value_editor(
    ui: &mut Ui,
    editors: &mut ValueEditors,
    default: &mut Value,
    schema: &Schema,
    ty: Option<&Ty>,
    path: &str,
) {
    let enum_values = match ty {
        Some(Ty::Named(n)) => schema
            .type_index(n)
            .and_then(|i| match &schema.types[i].body {
                Body::Enum { values } => Some(values.clone()),
                Body::Struct { .. } => None,
            }),
        _ => None,
    };
    let json = |ui: &mut Ui, default: &mut Value, salt: &str| {
        if let Some(new) = commit_field(ui, salt, &default.to_string(), DEFAULT_W, "")
            && let Ok(v) = serde_json::from_str::<Value>(new.trim())
        {
            *default = v;
        }
    };
    if let (Some((_, fields)), Some(object)) = (struct_of(schema, ty), default.as_object()) {
        let set = fields
            .iter()
            .filter(|f| object.contains_key(&f.name))
            .count();
        ui.label(RichText::new(format!("{set} of {} set", fields.len())).color(TEXT_WEAK));
        return;
    }
    let key = (editors.ti, path.to_owned());
    match (ty, enum_values) {
        (_, Some(values)) => {
            let current = default.as_str().unwrap_or_default().to_owned();
            egui::ComboBox::from_id_salt("default")
                .width(DEFAULT_W - 8.0)
                .selected_text(&current)
                .show_ui(ui, |ui| {
                    for v in values {
                        if ui.selectable_label(v == current, &v).clicked() {
                            *default = Value::String(v);
                        }
                    }
                });
        }
        (Some(Ty::Prim("bool")), _) => {
            let mut on = default.as_bool().unwrap_or_default();
            if ui.checkbox(&mut on, "").changed() {
                *default = on.into();
            }
        }
        (Some(ty), _) if ty.is_number() => {
            json(ui, default, "default");
            if !default.is_number() {
                *default = 0.into();
            }
        }
        (Some(Ty::Prim("string")), _) => {
            let mut text = default.as_str().unwrap_or_default().to_owned();
            if ui
                .add_sized(vec2(DEFAULT_W, 22.0), egui::TextEdit::singleline(&mut text))
                .changed()
            {
                *default = Value::String(text);
            }
        }
        (Some(Ty::Prim(p @ ("rgb" | "rgba"))), _) => {
            let alpha = *p == "rgba";
            let color = default.as_str().and_then(gradient::parse_color);
            let (swatch, response) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
            color_picker::checker(ui, swatch, 3);
            if let Some(c) = color {
                ui.painter()
                    .rect_filled(swatch, 3, color_picker::to_color32(c));
            }
            popover(
                &response.on_hover_text("Double-click to pick"),
                240.0,
                |ui| {
                    let mut c = color.unwrap_or([0, 0, 0, 255]);
                    if editors.picker.show(ui, &mut c) {
                        let [r, g, b, a] = c;
                        *default = if alpha {
                            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
                        } else {
                            format!("#{r:02x}{g:02x}{b:02x}")
                        }
                        .into();
                    }
                },
            );
            let current = default.as_str().unwrap_or_default().to_owned();
            if let Some(new) = commit_field(ui, "default", &current, DEFAULT_W - 22.0, "") {
                *default = Value::String(new.trim().to_owned());
            }
        }
        (Some(Ty::Prim(p @ ("curve" | "gradient"))), _) => {
            let (rect, response) = ui.allocate_exact_size(vec2(60.0, 18.0), Sense::click());
            let response = response.on_hover_text("Double-click to edit");
            if *p == "curve" {
                curve_preview(ui.painter(), rect, default);
                popover(&response, curve::WIDTH, |ui| {
                    let state = editors.curves.entry(key).or_default();
                    curve::curve_editor(ui, default, state, editors.curve_settings);
                });
            } else {
                gradient_preview(ui, rect, default);
                ui.painter()
                    .rect_stroke(rect, 3, Stroke::new(1.0, STROKE), StrokeKind::Inside);
                popover(&response, curve::WIDTH, |ui| {
                    gradient_default(ui, default, editors.gradients.entry(key).or_default());
                });
            }
        }
        _ => json(ui, default, "default"),
    }
}

/// The fields of struct `si` in a struct value: one row each with a Set checkbox and,
/// ticked, the value's editor; nested structs under their rows. `shown` are the structs
/// being shown around it. See docs/features/schemas.md#defaults.
fn struct_value(
    ui: &mut Ui,
    editors: &mut ValueEditors,
    value: &mut Value,
    schema: &Schema,
    si: usize,
    path: &str,
    shown: &mut Vec<usize>,
) {
    let Body::Struct { fields, .. } = &schema.types[si].body else {
        return;
    };
    let Some(object) = value.as_object_mut() else {
        return;
    };
    shown.push(si);
    for f in fields {
        let ty = parse_type(&f.ty).ok();
        let path = format!("{path}/{}", f.name);
        ui.push_id(&path, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                let cell = |ui: &mut Ui, text: &str, width: f32, color| {
                    ui.allocate_ui_with_layout(
                        vec2(width, 22.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(width);
                            let text = RichText::new(text).monospace().color(color);
                            ui.add(egui::Label::new(text).truncate());
                        },
                    );
                };
                // Under the field row's grip and settings icon, name and type (and its
                // menu button).
                ui.add_space(42.0);
                cell(ui, &f.name, NAME_W, TEXT);
                cell(ui, &f.ty, TYPE_W + 30.0, TEXT_WEAK);
                let mut set = object.contains_key(&f.name);
                if ui.checkbox(&mut set, "Set").changed() {
                    if set {
                        let start = f.default.clone().unwrap_or_else(|| {
                            ty.as_ref()
                                .map_or(Value::Null, |ty| aschema::implicit_value(schema, ty))
                        });
                        object.insert(f.name.clone(), start);
                    } else {
                        object.shift_remove(&f.name);
                    }
                }
                if let Some(v) = object.get_mut(&f.name) {
                    match struct_of(schema, ty.as_ref()) {
                        // A struct around this one again: an embedding loop, edited as text.
                        Some((inner, _)) if shown.contains(&inner) => {
                            value_editor(ui, editors, v, schema, None, &path);
                        }
                        _ => value_editor(ui, editors, v, schema, ty.as_ref(), &path),
                    }
                }
            });
            if let Some((inner, _)) = struct_of(schema, ty.as_ref())
                && !shown.contains(&inner)
                && let Some(v) = object.get_mut(&f.name)
                && v.is_object()
            {
                ui.indent("fields", |ui| {
                    struct_value(ui, editors, v, schema, inner, &path, shown);
                });
            }
        });
    }
    shown.pop();
}

/// A gradient default in the gradient editor.
fn gradient_default(ui: &mut Ui, value: &mut Value, card: &mut GradientCard) {
    let stops = match aschema::gradient_stops(value) {
        Ok(stops) => stops,
        Err(e) => {
            ui.label(RichText::new(e).color(CHANNEL_R));
            if ui.button("Reset").clicked() {
                *value = aschema::default_gradient();
            }
            return;
        }
    };
    let mut g = Gradient {
        name: String::new(),
        dither: Dither::None,
        stops: stops
            .into_iter()
            .map(|(at, color)| Stop { at, color })
            .collect(),
    };
    let before = g.stops.clone();
    ui.push_id("gradient_default", |ui| gradient_editor(ui, &mut g, card));
    if g.stops != before {
        *value = Value::Array(
            g.stops
                .iter()
                .map(|s| {
                    let [r, gr, b, a] = s.color;
                    Value::Array(vec![
                        aschema::json_number(s.at),
                        format!("#{r:02x}{gr:02x}{b:02x}{a:02x}").into(),
                    ])
                })
                .collect(),
        );
    }
}

fn gradient_preview(ui: &Ui, rect: Rect, value: &Value) {
    color_picker::checker(ui, rect, 3);
    let Ok(stops) = aschema::gradient_stops(value) else {
        return;
    };
    let stops: Vec<Stop> = stops
        .into_iter()
        .map(|(at, color)| Stop { at, color })
        .collect();
    let slices = 30;
    for i in 0..slices {
        let t = (i as f64 + 0.5) / slices as f64;
        let x0 = rect.left() + rect.width() * i as f32 / slices as f32;
        let x1 = rect.left() + rect.width() * (i + 1) as f32 / slices as f32;
        let slice = Rect::from_min_max(pos2(x0, rect.top()), pos2(x1, rect.bottom()));
        let color = color_picker::to_color32(gradient::color_at(&stops, t));
        ui.painter().rect_filled(slice, 0, color);
    }
}

/// A field's details: only what its type can use; returns whether it moved to another group.
/// See docs/features/schemas.md#struct.
fn field_details(
    ui: &mut Ui,
    edit: &mut SchemaEdit,
    ti: usize,
    fi: usize,
    ty: Option<&Ty>,
    schema: &Schema,
    groups: &[String],
) -> bool {
    let Body::Struct { fields, .. } = &mut edit.edit.types[ti].body else {
        return false;
    };
    let field = &mut fields[fi];
    let mut regroup = false;
    ui.set_max_width(560.0);
    let text = |ui: &mut Ui, label: &str, value: &mut String, mono: bool| {
        prop_row(ui, label, |ui, width| {
            let mut edit = egui::TextEdit::singleline(value);
            if mono {
                edit = edit.font(egui::TextStyle::Monospace);
            }
            ui.add_sized(vec2(width, 22.0), edit);
        });
    };
    text(ui, "Label", &mut field.label, false);
    text(ui, "Doc", &mut field.doc, false);
    if ty.is_some_and(Ty::is_number) {
        for (label, value) in [
            ("Min", &mut field.min),
            ("Max", &mut field.max),
            ("Step", &mut field.step),
        ] {
            prop_row(ui, label, |ui, width| {
                if let Some(new) = number_field(ui, label, value.as_ref(), width) {
                    *value = new;
                }
            });
        }
    }
    // The one widget the field's type can use, or the one it has.
    let target = match ty {
        Some(Ty::Named(n)) => schema.type_index(n).map(|i| &schema.types[i].body),
        _ => None,
    };
    let fits = match target {
        Some(Body::Enum { .. }) => "toggle",
        Some(Body::Struct { .. }) => "inline",
        None => "",
    };
    if !fits.is_empty() || !field.widget.is_empty() {
        prop_row(ui, "Widget", |ui, width| {
            let shown = |w: &str| {
                if w.is_empty() {
                    "—".to_owned()
                } else {
                    w.to_owned()
                }
            };
            egui::ComboBox::from_id_salt("widget")
                .width(width - 8.0)
                .selected_text(shown(&field.widget))
                .show_ui(ui, |ui| {
                    let mut choices = vec![String::new()];
                    for w in [fits, field.widget.as_str()] {
                        if !w.is_empty() && !choices.iter().any(|c| c == w) {
                            choices.push(w.to_owned());
                        }
                    }
                    for w in choices {
                        let text = shown(&w);
                        ui.selectable_value(&mut field.widget, w, text);
                    }
                });
        });
    }
    prop_row(ui, "Group", |ui, width| {
        let shown = |g: &str| {
            if g.is_empty() {
                "—".to_owned()
            } else {
                g.to_owned()
            }
        };
        egui::ComboBox::from_id_salt("group")
            .width(width - 8.0)
            .selected_text(shown(&field.group))
            .show_ui(ui, |ui| {
                for g in std::iter::once("").chain(groups.iter().map(String::as_str)) {
                    if ui.selectable_label(field.group == g, shown(g)).clicked() && field.group != g
                    {
                        field.group = g.to_owned();
                        regroup = true;
                    }
                }
            });
    });
    prop_row(ui, "Order", |ui, width| {
        if let Some(new) = number_field(ui, "order", field.order.as_ref(), width) {
            field.order = new;
        }
    });
    prop_row(ui, "Readonly", |ui, _| {
        ui.checkbox(&mut field.readonly, "");
    });
    text(ui, "Show if", &mut field.show_if, true);
    text(ui, "Hide if", &mut field.hide_if, true);
    regroup
}

/// Values of enum `ti`: reorder, rename, add and remove; see docs/features/schemas.md#enum.
fn enum_editor(ui: &mut Ui, edit: &mut SchemaEdit, ti: usize) {
    let Body::Enum { values } = &mut edit.edit.types[ti].body else {
        return;
    };
    ui.label(RichText::new("Values").size(14.0).color(TEXT));
    ui.add_space(4.0);
    let dragging = Cell::new(None);
    let dropped = Cell::new(None);
    let mut remove = None;
    let mut rects = Vec::new();
    let removable = values.len() > 1;
    for vi in 0..values.len() {
        let names = values.clone();
        let row = ui
            .push_id(("value", vi), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let grip = drag_handle(ui);
                    if grip.dragged() {
                        dragging.set(Some(vi));
                    }
                    if grip.drag_stopped() {
                        dropped.set(Some(vi));
                    }
                    if let Some(new) = commit_field(ui, "value", &values[vi], 200.0, "value")
                        && !new.is_empty()
                        && !names.iter().enumerate().any(|(k, n)| k != vi && *n == new)
                    {
                        values[vi] = new;
                    }
                    if icon_button(ui, Icon::X, "Remove value", removable).clicked() {
                        remove = Some(vi);
                    }
                })
                .response
                .rect
            })
            .inner;
        rects.push(row);
    }
    if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
        let v = values.remove(from);
        values.insert(to, v);
    }
    if let Some(vi) = remove {
        values.remove(vi);
    }
    ui.add_space(4.0);
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add value");
    if ui.add(add).clicked() {
        let name = free_name("value", |n| values.iter().any(|v| v == n));
        values.push(name);
    }
}
