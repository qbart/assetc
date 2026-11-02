//! The `.aschema` editor: the Inspector's Types tab and the canvas; see
//! docs/features/schemas.md.

use std::cell::Cell;

use serde_json::{Number, Value};

use super::*;
use crate::aschema::{Field, Group, Problem, Ty, TypeDef, parse_type};
use crate::theme::CHANNEL_R;

/// A schema being edited.
#[derive(Default)]
pub(super) struct SchemaEdit {
    pub(super) edit: Schema,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    pub(super) saved: Schema,
    /// Unfolded field rows, by type and field.
    open: HashSet<(usize, usize)>,
    /// Curve and gradient editors of defaults, by type and field.
    curves: HashMap<(usize, usize), CurveState>,
    gradients: HashMap<(usize, usize), GradientCard>,
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
                    if problems.iter().any(|p| p.type_index == i) {
                        painter.circle_filled(
                            pos2(size.left() - 8.0, rect.center().y),
                            3.0,
                            CHANNEL_R,
                        );
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
        type_header(ui, edit, ti);
        ui.add_space(12.0);
        match edit.edit.types[ti].body {
            Body::Struct { .. } => struct_editor(ui, edit, &snapshot, ti, &problems),
            Body::Enum { .. } => enum_editor(ui, edit, ti),
        }
        // Every problem of the file; clicking one chooses its type.
        ui.add_space(16.0);
        ui.label(RichText::new("Problems").size(14.0).color(TEXT));
        if problems.is_empty() {
            ui.weak("None");
        }
        for p in &problems {
            let text = RichText::new(format!("{}: {}", p.path, p.message))
                .size(12.0)
                .color(CHANNEL_R);
            if ui
                .add(egui::Label::new(text).sense(Sense::click()))
                .clicked()
            {
                *chosen = p.type_index;
            }
        }
    });
    schema_commit(ui, edit)
}

/// Kind, name and doc of type `ti`.
fn type_header(ui: &mut Ui, edit: &mut SchemaEdit, ti: usize) {
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
        let doc = &mut edit.edit.types[ti].doc;
        let field = egui::TextEdit::singleline(doc).hint_text("Doc");
        ui.add_sized(vec2(ui.available_width().max(120.0), 22.0), field);
    });
}

/// Column widths of the fields table: name, type, default, min/max/step, group, label.
const NAME_W: f32 = 110.0;
const TYPE_W: f32 = 130.0;
const DEFAULT_W: f32 = 130.0;
const NUMBER_W: f32 = 50.0;
const TEXT_W: f32 = 80.0;
const DOC_W: f32 = 100.0;

/// The fields table, groups and Add buttons of struct `ti`; see
/// docs/features/schemas.md#struct.
fn struct_editor(
    ui: &mut Ui,
    edit: &mut SchemaEdit,
    snapshot: &Schema,
    ti: usize,
    problems: &[Problem],
) {
    ui.label(RichText::new("Fields").size(14.0).color(TEXT));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let head = |ui: &mut Ui, text: &str, width: f32| {
            ui.add_sized(
                vec2(width, 18.0),
                egui::Label::new(RichText::new(text).size(12.0).color(TEXT_WEAK)),
            );
        };
        head(ui, "", 24.0);
        head(ui, "Name", NAME_W);
        head(ui, "Type", TYPE_W + 26.0);
        head(ui, "Default", DEFAULT_W);
        head(ui, "Min", NUMBER_W);
        head(ui, "Max", NUMBER_W);
        head(ui, "Step", NUMBER_W);
        head(ui, "Group", TEXT_W);
        head(ui, "Label", TEXT_W);
        head(ui, "Doc", (ui.available_width() - 60.0).max(DOC_W));
    });
    let dragging = Cell::new(None);
    let dropped = Cell::new(None);
    let mut delete = None;
    let mut rects = Vec::new();
    let count = match &edit.edit.types[ti].body {
        Body::Struct { fields, .. } => fields.len(),
        Body::Enum { .. } => 0,
    };
    for fi in 0..count {
        let open = edit.open.contains(&(ti, fi));
        let mut toggle = false;
        let names: Vec<String> = match &edit.edit.types[ti].body {
            Body::Struct { fields, .. } => fields.iter().map(|f| f.name.clone()).collect(),
            Body::Enum { .. } => Vec::new(),
        };
        let Body::Struct { fields, .. } = &mut edit.edit.types[ti].body else {
            return;
        };
        let field = &mut fields[fi];
        let ty = parse_type(&field.ty).ok();
        let row = ui
            .push_id(("field", fi), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let grip = drag_handle(ui);
                    if grip.dragged() {
                        dragging.set(Some(fi));
                    }
                    if grip.drag_stopped() {
                        dropped.set(Some(fi));
                    }
                    if let Some(new) = commit_field(ui, "name", &field.name, NAME_W, "name")
                        && aschema::is_identifier(&new)
                        && !names.iter().enumerate().any(|(k, n)| k != fi && *n == new)
                    {
                        field.name = new;
                    }
                    type_cell(ui, field, snapshot, ty.is_some());
                    toggle |= default_cell(ui, field, snapshot, ty.as_ref());
                    let numeric = ty.as_ref().is_some_and(Ty::is_number);
                    ui.add_enabled_ui(numeric, |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (salt, value) in [
                            ("min", &mut field.min),
                            ("max", &mut field.max),
                            ("step", &mut field.step),
                        ] {
                            if let Some(new) = number_field(ui, salt, value.as_ref(), NUMBER_W) {
                                *value = new;
                            }
                        }
                    });
                    let text = |ui: &mut Ui, value: &mut String, width: f32| {
                        ui.add_sized(vec2(width, 22.0), egui::TextEdit::singleline(value));
                    };
                    text(ui, &mut field.group, TEXT_W);
                    text(ui, &mut field.label, TEXT_W);
                    // Doc takes what the two buttons leave.
                    let doc = (ui.available_width() - 2.0 * 30.0).max(DOC_W);
                    text(ui, &mut field.doc, doc);
                    let chevron = if open {
                        Icon::ChevronDown
                    } else {
                        Icon::ChevronRight
                    };
                    toggle |= icon_button(ui, chevron, "More", true).clicked();
                    if icon_button(ui, Icon::X, "Remove field", true).clicked() {
                        delete = Some(fi);
                    }
                })
                .response
                .rect
            })
            .inner;
        rects.push(row);
        if open {
            ui.push_id(("details", fi), |ui| {
                ui.indent("details", |ui| {
                    field_details(ui, edit, ti, fi, ty.as_ref(), snapshot);
                });
            });
        }
        for p in problems
            .iter()
            .filter(|p| p.type_index == ti && p.field == Some(fi))
        {
            ui.horizontal(|ui| {
                ui.add_space(30.0);
                ui.label(RichText::new(&p.message).size(12.0).color(CHANNEL_R));
            });
        }
        if toggle {
            if open {
                edit.open.remove(&(ti, fi));
            } else {
                edit.open.insert((ti, fi));
            }
        }
    }
    let Body::Struct { fields, groups } = &mut edit.edit.types[ti].body else {
        return;
    };
    if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
        let f = fields.remove(from);
        fields.insert(to, f);
        edit.open.clear();
    }
    if let Some(fi) = delete {
        fields.remove(fi);
        edit.open.clear();
    }
    ui.add_space(4.0);
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add field");
    if ui.add(add).clicked() {
        let name = free_name("field", |n| fields.iter().any(|f| f.name == n));
        fields.push(Field::new(name, "string"));
    }

    // Groups; see docs/features/schemas.md#struct.
    ui.add_space(12.0);
    ui.label(RichText::new("Groups").size(14.0).color(TEXT));
    let mut remove = None;
    for gi in 0..groups.len() {
        let names: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
        let group = &mut groups[gi];
        ui.push_id(("group", gi), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if let Some(new) = commit_field(ui, "name", &group.name, NAME_W, "name")
                    && !new.is_empty()
                    && !names.iter().enumerate().any(|(k, n)| k != gi && *n == new)
                {
                    group.name = new;
                }
                ui.checkbox(&mut group.collapsed, "Collapsed");
                let doc = egui::TextEdit::singleline(&mut group.doc).hint_text("Doc");
                ui.add_sized(vec2(DOC_W * 2.0, 22.0), doc);
                if icon_button(ui, Icon::X, "Remove group", true).clicked() {
                    remove = Some(gi);
                }
            });
        });
    }
    if let Some(gi) = remove {
        groups.remove(gi);
    }
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add group");
    if ui.add(add).clicked() {
        let name = free_name("Group", |n| groups.iter().any(|g| g.name == n));
        groups.push(Group {
            name,
            ..Group::default()
        });
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
    let button = egui::Button::image(Icon::ChevronDown.image(12.0, TEXT_WEAK)).frame(false);
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        let current = field.ty.clone();
        let mut pick = |ui: &mut Ui, text: String| {
            if menu_item(ui, &text, None).clicked() {
                field.ty = text;
                ui.close();
            }
        };
        for generic in aschema::GENERICS {
            pick(ui, format!("{generic}<{current}>"));
        }
        ui.separator();
        for p in aschema::PRIMITIVES
            .into_iter()
            .chain(std::iter::once(aschema::GRADIENT))
        {
            pick(ui, p.to_owned());
        }
        if !schema.types.is_empty() {
            ui.separator();
        }
        for t in &schema.types {
            pick(ui, t.name.clone());
        }
    });
}

/// Whether every name in `ty` is a primitive or a type of the schema.
fn check_known(schema: &Schema, ty: &Ty) -> bool {
    match ty {
        Ty::Prim(_) => true,
        Ty::Named(n) => schema.type_index(n).is_some(),
        Ty::List(t) | Ty::Optional(t) | Ty::Ref(t) => check_known(schema, t),
    }
}

/// The Default cell of a field; returns whether it asks to unfold the row. See
/// docs/features/schemas.md#defaults.
fn default_cell(ui: &mut Ui, field: &mut Field, schema: &Schema, ty: Option<&Ty>) -> bool {
    let hint = ty
        .map(|t| aschema::implicit_default(schema, t))
        .unwrap_or_default();
    let enum_values = match ty {
        Some(Ty::Named(n)) => schema
            .type_index(n)
            .and_then(|i| match &schema.types[i].body {
                Body::Enum { values } => Some(values.clone()),
                Body::Struct { .. } => None,
            }),
        _ => None,
    };
    let shown = |v: &Option<Value>| match v {
        None => "—".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    let mut unfold = false;
    match (ty, enum_values) {
        (Some(Ty::Prim("bool")), _) | (_, Some(_)) if ty.is_some() => {
            let choices: Vec<Option<Value>> = match ty {
                Some(Ty::Prim("bool")) => vec![None, Some(false.into()), Some(true.into())],
                _ => std::iter::once(None)
                    .chain(
                        match ty {
                            Some(Ty::Named(n)) => {
                                match schema.type_index(n).map(|i| &schema.types[i].body) {
                                    Some(Body::Enum { values }) => values.clone(),
                                    _ => Vec::new(),
                                }
                            }
                            _ => Vec::new(),
                        }
                        .into_iter()
                        .map(|v| Some(Value::String(v))),
                    )
                    .collect(),
            };
            egui::ComboBox::from_id_salt("default")
                .width(DEFAULT_W - 8.0)
                .selected_text(shown(&field.default))
                .show_ui(ui, |ui| {
                    for choice in choices {
                        let text = shown(&choice);
                        ui.selectable_value(&mut field.default, choice, text);
                    }
                });
        }
        (Some(ty), _) if ty.is_number() => {
            let current = field
                .default
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_default();
            if let Some(new) = commit_field(ui, "default", &current, DEFAULT_W, &hint) {
                let new = new.trim();
                if new.is_empty() {
                    field.default = None;
                } else if let Ok(v) = serde_json::from_str::<Value>(new)
                    && v.is_number()
                {
                    field.default = Some(v);
                }
            }
        }
        (Some(Ty::Prim(p @ ("string" | "rgb" | "rgba"))), _) => {
            let mut width = DEFAULT_W;
            if *p != "string" {
                let color = field
                    .default
                    .as_ref()
                    .and_then(Value::as_str)
                    .and_then(crate::gradient::parse_color);
                let (swatch, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                color_picker::checker(ui, swatch, 3);
                if let Some(c) = color {
                    ui.painter()
                        .rect_filled(swatch, 3, color_picker::to_color32(c));
                }
                width -= 22.0;
            }
            let current = field
                .default
                .as_ref()
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(new) = commit_field(ui, "default", &current, width, &hint) {
                field.default = (!new.is_empty()).then(|| Value::String(new));
            }
        }
        (Some(Ty::Prim(p @ ("curve" | "gradient"))), _) => {
            let (rect, response) = ui.allocate_exact_size(vec2(DEFAULT_W, 22.0), Sense::click());
            let preview = Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(60.0, 18.0));
            let painter = ui.painter();
            match &field.default {
                None => {
                    painter.text(
                        preview.left_center(),
                        Align2::LEFT_CENTER,
                        "—",
                        FontId::proportional(14.0),
                        TEXT_WEAK,
                    );
                }
                Some(v) if *p == "curve" => curve_preview(painter, preview, v),
                Some(v) => gradient_preview(ui, preview, v),
            }
            unfold = response
                .on_hover_text("Edit in the row's details")
                .clicked();
        }
        _ => {
            let current = field
                .default
                .as_ref()
                .map(Value::to_string)
                .unwrap_or_default();
            if let Some(new) = commit_field(ui, "default", &current, DEFAULT_W, &hint) {
                if new.trim().is_empty() {
                    field.default = None;
                } else if let Ok(v) = serde_json::from_str::<Value>(&new) {
                    field.default = Some(v);
                }
            }
        }
    }
    unfold
}

fn curve_preview(painter: &egui::Painter, rect: Rect, value: &Value) {
    painter.rect_filled(rect, 3, BG_PANEL);
    let Ok(points) = aschema::curve_points(value) else {
        return;
    };
    let samples: Vec<f32> = (0..=30)
        .map(|i| aschema::evaluate_curve(&points, i as f32 / 30.0))
        .collect();
    let lo = samples.iter().copied().fold(0.0f32, f32::min);
    let hi = samples.iter().copied().fold(1.0f32, f32::max);
    let line: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, y)| {
            pos2(
                rect.left() + 2.0 + i as f32 / 30.0 * (rect.width() - 4.0),
                rect.bottom() - 2.0 - (y - lo) / (hi - lo).max(1e-6) * (rect.height() - 4.0),
            )
        })
        .collect();
    painter.add(egui::Shape::line(line, Stroke::new(1.5, ACCENT)));
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

/// An unfolded field row: layout keys, and the curve or gradient default's editor.
fn field_details(
    ui: &mut Ui,
    edit: &mut SchemaEdit,
    ti: usize,
    fi: usize,
    ty: Option<&Ty>,
    schema: &Schema,
) {
    let Body::Struct { fields, .. } = &mut edit.edit.types[ti].body else {
        return;
    };
    let field = &mut fields[fi];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let weak = |ui: &mut Ui, text: &str| {
            ui.label(RichText::new(text).color(TEXT_WEAK));
        };
        weak(ui, "Order");
        if let Some(new) = number_field(ui, "order", field.order.as_ref(), NUMBER_W) {
            field.order = new;
        }
        ui.checkbox(&mut field.readonly, "Readonly");
        weak(ui, "Show if");
        ui.add_sized(
            vec2(140.0, 22.0),
            egui::TextEdit::singleline(&mut field.show_if).font(egui::TextStyle::Monospace),
        );
        weak(ui, "Hide if");
        ui.add_sized(
            vec2(140.0, 22.0),
            egui::TextEdit::singleline(&mut field.hide_if).font(egui::TextStyle::Monospace),
        );
        weak(ui, "Widget");
        egui::ComboBox::from_id_salt("widget")
            .width(80.0)
            .selected_text(if field.widget.is_empty() {
                "—"
            } else {
                field.widget.as_str()
            })
            .show_ui(ui, |ui| {
                for w in ["", "toggle", "inline"] {
                    let text = if w.is_empty() { "—" } else { w };
                    ui.selectable_value(&mut field.widget, w.to_owned(), text);
                }
            });
    });
    let _ = schema;
    let kind = match ty {
        Some(Ty::Prim(p @ ("curve" | "gradient"))) => *p,
        _ => return,
    };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Default").color(TEXT_WEAK));
        if field.default.is_none() {
            if ui.button("Set").clicked() {
                field.default = Some(if kind == "curve" {
                    aschema::linear_curve()
                } else {
                    aschema::default_gradient()
                });
            }
        } else if ui.button("Clear").clicked() {
            field.default = None;
        }
    });
    let Some(value) = &mut field.default else {
        return;
    };
    ui.set_max_width(560.0);
    if kind == "curve" {
        curve_editor(ui, value, edit.curves.entry((ti, fi)).or_default());
    } else {
        let card = edit.gradients.entry((ti, fi)).or_default();
        match aschema::gradient_stops(value) {
            Ok(stops) => {
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
            Err(e) => {
                ui.label(RichText::new(e).color(CHANNEL_R));
                if ui.button("Reset").clicked() {
                    *value = aschema::default_gradient();
                }
            }
        }
    }
}

/// The curve editor's state.
#[derive(Default)]
struct CurveState {
    point: Option<usize>,
    drag: Option<CurveDrag>,
}

#[derive(Clone, Copy)]
enum CurveDrag {
    Point(usize),
    /// A tangent handle of a point: out (true) or in, and whether the tangents are broken.
    Handle(usize, bool, bool),
}

/// Points, tangent handles and the chosen point's fields; see
/// docs/features/schemas.md#curve-editor.
fn curve_editor(ui: &mut Ui, value: &mut Value, state: &mut CurveState) {
    const REACH: f32 = 6.0;
    const HANDLE: f32 = 40.0;
    let mut points = match aschema::curve_points(value) {
        Ok(points) if !points.is_empty() => points,
        Ok(_) => vec![[0.0, 0.0, 0.0, 0.0]],
        Err(e) => {
            ui.label(RichText::new(e).color(CHANNEL_R));
            if ui.button("Reset").clicked() {
                *value = aschema::linear_curve();
            }
            return;
        }
    };
    let before = points.clone();
    state.point = state.point.filter(|&k| k < points.len());
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 140.0), Sense::click_and_drag());
    let inner = rect.shrink(8.0);
    // The vertical range: the curve's, at least 0..1, plus 10%.
    let samples: Vec<f32> = (0..=64)
        .map(|i| aschema::evaluate_curve(&points, i as f32 / 64.0))
        .chain(points.iter().map(|p| p[1]))
        .collect();
    let mut lo = samples.iter().copied().fold(0.0f32, f32::min);
    let mut hi = samples.iter().copied().fold(1.0f32, f32::max);
    let pad = (hi - lo) * 0.1;
    (lo, hi) = (lo - pad, hi + pad);
    let to_screen = |x: f32, y: f32| {
        pos2(
            inner.left() + x * inner.width(),
            inner.bottom() - (y - lo) / (hi - lo) * inner.height(),
        )
    };
    let from_screen = |p: egui::Pos2| {
        (
            (p.x - inner.left()) / inner.width(),
            lo + (inner.bottom() - p.y) / inner.height() * (hi - lo),
        )
    };
    let (sx, sy) = (inner.width(), inner.height() / (hi - lo));
    // A handle's end for slope `m`, to the right (out) or left (in) of the point.
    let handle_end = |p: [f32; 4], out: bool| {
        let m = if out { p[3] } else { p[2] };
        let dir = vec2(sx, -m * sy).normalized() * HANDLE;
        to_screen(p[0], p[1]) + if out { dir } else { -dir }
    };
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let near_point = |points: &[[f32; 4]], at: egui::Pos2| {
        points
            .iter()
            .enumerate()
            .map(|(k, p)| (k, (to_screen(p[0], p[1]) - at).length()))
            .filter(|(_, d)| *d <= REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    };
    if response.double_clicked()
        && let Some(at) = pointer
    {
        match near_point(&points, at) {
            Some(k) if points.len() > 1 => {
                points.remove(k);
                state.point = None;
            }
            Some(_) => {}
            None => {
                let x = from_screen(at).0.clamp(0.0, 1.0);
                let y = aschema::evaluate_curve(&points, x);
                let h = 1e-3;
                let slope = (aschema::evaluate_curve(&points, (x + h).min(1.0))
                    - aschema::evaluate_curve(&points, (x - h).max(0.0)))
                    / (2.0 * h);
                let k = points.partition_point(|p| p[0] <= x);
                points.insert(k, [x, y, slope, slope]);
                state.point = Some(k);
            }
        }
        state.drag = None;
    } else if ui.input(|i| i.pointer.primary_pressed())
        && response.hovered()
        && let Some(at) = pointer
    {
        let shift = ui.input(|i| i.modifiers.shift);
        let handle = state.point.and_then(|k| {
            [true, false]
                .into_iter()
                .find(|&out| (handle_end(points[k], out) - at).length() <= REACH)
                .map(|out| {
                    let broken = shift || (points[k][2] - points[k][3]).abs() > 1e-4;
                    CurveDrag::Handle(k, out, broken)
                })
        });
        state.drag = handle.or_else(|| near_point(&points, at).map(CurveDrag::Point));
        if let Some(CurveDrag::Point(k)) = state.drag {
            state.point = Some(k);
        }
    }
    match (state.drag, ui.input(|i| i.pointer.primary_down()), pointer) {
        (Some(CurveDrag::Point(k)), true, Some(at)) if k < points.len() => {
            let (x, y) = from_screen(at);
            let left = if k > 0 { points[k - 1][0] } else { 0.0 };
            let right = points.get(k + 1).map_or(1.0, |p| p[0]);
            points[k][0] = x.clamp(left, right).clamp(0.0, 1.0);
            points[k][1] = y;
        }
        (Some(CurveDrag::Handle(k, out, broken)), true, Some(at)) if k < points.len() => {
            let center = to_screen(points[k][0], points[k][1]);
            let dx = (at.x - center.x) / sx;
            let dy = -(at.y - center.y) / sy;
            // Toward its own side; straight up or down is steep, not infinite.
            let dx = if out { dx.max(1e-3) } else { dx.min(-1e-3) };
            let slope = (dy / dx).clamp(-1e3, 1e3);
            let slot = if out { 3 } else { 2 };
            points[k][slot] = slope;
            if !broken {
                points[k][5 - slot] = slope;
            }
        }
        _ => state.drag = None,
    }

    // Drawing.
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4, BG_PANEL);
    painter.rect_stroke(rect, 4, Stroke::new(1.0, STROKE), StrokeKind::Inside);
    for x in [0.25, 0.5, 0.75] {
        painter.vline(
            to_screen(x, 0.0).x,
            inner.y_range(),
            Stroke::new(1.0, STROKE),
        );
    }
    for y in [0.0, 1.0] {
        painter.hline(
            inner.x_range(),
            to_screen(0.0, y).y,
            Stroke::new(1.0, STROKE),
        );
    }
    let line: Vec<egui::Pos2> = (0..=128)
        .map(|i| {
            let x = i as f32 / 128.0;
            to_screen(x, aschema::evaluate_curve(&points, x))
        })
        .collect();
    painter.add(egui::Shape::line(line, Stroke::new(2.0, ACCENT)));
    for (k, p) in points.iter().enumerate() {
        let at = to_screen(p[0], p[1]);
        let chosen = state.point == Some(k);
        if chosen {
            for out in [false, true] {
                let end = handle_end(*p, out);
                painter.line_segment([at, end], Stroke::new(1.0, TEXT_WEAK));
                painter.circle_filled(end, 3.0, TEXT);
            }
        }
        let color = if chosen { ACCENT } else { TEXT };
        painter.rect_filled(Rect::from_center_size(at, vec2(8.0, 8.0)), 1, color);
    }

    // The chosen point's numbers.
    if let Some(k) = state.point {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let left = if k > 0 { points[k - 1][0] } else { 0.0 };
            let right = points.get(k + 1).map_or(1.0, |p| p[0]);
            for (i, label) in ["X", "Y", "In", "Out"].into_iter().enumerate() {
                ui.label(RichText::new(label).color(TEXT_WEAK));
                let mut field = egui::DragValue::new(&mut points[k][i])
                    .speed(0.01)
                    .fixed_decimals(3);
                if i == 0 {
                    field = field.range(left..=right);
                }
                ui.add_sized(vec2(64.0, 22.0), field);
            }
        });
    }
    if points != before {
        *value = Value::Array(
            points
                .iter()
                .map(|p| {
                    Value::Array(
                        p.iter()
                            .map(|&n| aschema::json_f32((n * 1e4).round() / 1e4))
                            .collect(),
                    )
                })
                .collect(),
        );
    }
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
