//! The `.adata` editor: the Inspector's Instances tab and the canvas; see
//! docs/features/instances.md.

use std::cell::Cell;

use serde_json::{Map, Value};

use super::curve::{CurveSettings, CurveState};
use super::schema::{ValueEditors, commit_field, nested_value, struct_of, value_editor};
use super::*;
use crate::adata::{self, DataFile, DataProblem, Instance, inherited, same};
use crate::aschema::{Field, Ty, parse_type};
use crate::theme::CHANNEL_R;

/// A field label's width, and a value editor's.
const LABEL_W: f32 = 160.0;
const VALUE_W: f32 = 240.0;

/// A data file being edited, with its schema.
pub(super) struct DataEdit {
    pub(super) edit: DataFile,
    /// As last read or written; `edit` is saved when it differs and nothing is being dragged
    /// or typed.
    pub(super) saved: DataFile,
    /// The schema `$schema` names, or why it can't be read.
    pub(super) schema: Result<Schema, String>,
    /// Every schema of the input directory, relative to the file's folder.
    pub(super) schemas: Vec<String>,
    /// The file's text, shown while the schema can't be read.
    pub(super) text: String,
    /// Folded groups, and unfolded struct values, by instance and path.
    folded: HashSet<(usize, String)>,
    open: HashSet<(usize, String)>,
    picker: Picker,
    curves: HashMap<(usize, String), CurveState>,
    curve_settings: CurveSettings,
    gradients: HashMap<(usize, String), GradientCard>,
    /// Groups were folded as `collapsed` says, by instance.
    started: HashSet<usize>,
}

/// What reading a data file gives the editor: the file, its schema, the schemas to choose
/// from and its text.
pub(super) type PreparedData = (DataFile, Result<Schema, String>, Vec<String>, String);

impl DataEdit {
    /// `data` as read, keeping `previous`'s editor state.
    pub(super) fn new(prepared: PreparedData, previous: Option<DataEdit>) -> Self {
        let (data, schema, schemas, text) = prepared;
        let base = Self {
            edit: data.clone(),
            saved: data,
            schema,
            schemas,
            text,
            folded: HashSet::new(),
            open: HashSet::new(),
            picker: Picker::default(),
            curves: HashMap::new(),
            curve_settings: CurveSettings::default(),
            gradients: HashMap::new(),
            started: HashSet::new(),
        };
        match previous {
            Some(p) => Self {
                folded: p.folded,
                open: p.open,
                picker: p.picker,
                curves: p.curves,
                curve_settings: p.curve_settings,
                gradients: p.gradients,
                started: p.started,
                ..base
            },
            None => base,
        }
    }
}

/// The text to write once nothing is being dragged or typed into, as for schemas.
fn data_commit(ui: &Ui, edit: &mut DataEdit) -> Option<String> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.to_json(edit.schema.as_ref().ok())
    })
}

/// The problem icon, red for errors and `ACCENT` for warnings only; hovering lists them.
fn issues(ui: &mut Ui, salt: impl std::hash::Hash + std::fmt::Debug, problems: &[&DataProblem]) {
    if problems.is_empty() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
    issues_at(
        ui,
        Rect::from_center_size(rect.center(), vec2(16.0, 16.0)),
        salt,
        problems,
    );
}

fn issues_at(
    ui: &Ui,
    rect: Rect,
    salt: impl std::hash::Hash + std::fmt::Debug,
    problems: &[&DataProblem],
) {
    let color = if problems.iter().all(|p| p.warning) {
        ACCENT
    } else {
        CHANNEL_R
    };
    Icon::Bug.image(16.0, color).paint_at(ui, rect);
    let text: Vec<String> = problems
        .iter()
        .map(|p| {
            if p.warning {
                format!("warning: {}", p.message)
            } else {
                p.message.clone()
            }
        })
        .collect();
    ui.interact(rect, ui.id().with(salt), Sense::hover())
        .on_hover_text(text.join("\n"));
}

/// The Instances tab: the schema, Add instance and the instances to choose from; returns
/// the text to write. See docs/features/instances.md#instances-tab.
pub(super) fn instances_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    chosen: &mut usize,
) -> Option<String> {
    let edit = match &mut inspected.data {
        Some(Ok(edit)) => edit,
        Some(Err((e, _))) => {
            placeholder(ui, e);
            return None;
        }
        None => {
            placeholder(ui, "No instances");
            return None;
        }
    };
    let problems = edit
        .schema
        .as_ref()
        .map(|s| edit.edit.check(s))
        .unwrap_or_default();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                prop_row(ui, "Schema", |ui, width| {
                    let shown = if edit.edit.schema.is_empty() {
                        "—"
                    } else {
                        edit.edit.schema.as_str()
                    };
                    egui::ComboBox::from_id_salt("data_schema")
                        .width(width - 8.0)
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for s in &edit.schemas {
                                ui.selectable_value(&mut edit.edit.schema, s.clone(), s);
                            }
                        });
                });
                ui.add_space(8.0);
                let structs: Vec<String> = edit
                    .schema
                    .as_ref()
                    .map(|s| {
                        s.types
                            .iter()
                            .filter(|t| matches!(t.body, Body::Struct { .. }))
                            .map(|t| t.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                ui.add_enabled_ui(!structs.is_empty(), |ui| {
                    let button =
                        egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add instance");
                    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                        for name in &structs {
                            if menu_item(ui, name, None).clicked() {
                                let id = edit.edit.free_id(name);
                                edit.edit.instances.push(Instance {
                                    ty: name.clone(),
                                    id,
                                    meta: Map::new(),
                                    values: Map::new(),
                                });
                                *chosen = edit.edit.instances.len() - 1;
                                ui.close();
                            }
                        }
                    });
                });
                ui.add_space(8.0);
                if edit.edit.instances.is_empty() {
                    ui.weak("No instances");
                }
                let dragging = Cell::new(None);
                let dropped = Cell::new(None);
                let mut delete = None;
                let mut rects = Vec::new();
                for (i, inst) in edit.edit.instances.iter().enumerate() {
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
                    let ty = Rect::from_min_size(
                        pos2(rect.left() + 30.0, rect.top()),
                        vec2(80.0, rect.height()),
                    );
                    painter.with_clip_rect(ty).text(
                        pos2(ty.left(), rect.center().y),
                        Align2::LEFT_CENTER,
                        &inst.ty,
                        FontId::proportional(11.0),
                        TEXT_WEAK,
                    );
                    painter.text(
                        pos2(rect.left() + 116.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        &inst.id,
                        FontId::proportional(14.0),
                        TEXT,
                    );
                    let mine: Vec<&DataProblem> =
                        problems.iter().filter(|p| p.instance == i).collect();
                    if !mine.is_empty() {
                        let right = if *chosen == i { 40.0 } else { 16.0 };
                        let at = Rect::from_center_size(
                            pos2(rect.right() - right, rect.center().y),
                            vec2(16.0, 16.0),
                        );
                        issues_at(ui, at, ("instance_problems", i), &mine);
                    }
                    if *chosen == i {
                        let at = Rect::from_min_size(
                            pos2(rect.right() - 26.0, rect.top() + 2.0),
                            vec2(24.0, 24.0),
                        );
                        let delete_clicked = ui
                            .scope_builder(UiBuilder::new().max_rect(at), |ui| {
                                icon_button(ui, Icon::Trash2, "Delete", true).clicked()
                            })
                            .inner;
                        if delete_clicked {
                            delete = Some(i);
                        }
                    } else if response.clicked() {
                        *chosen = i;
                    }
                }
                let instances = &mut edit.edit.instances;
                if let Some((from, to)) = card_reorder(ui, &rects, dragging.get(), dropped.get()) {
                    let moved = instances.remove(from);
                    instances.insert(to, moved);
                    if *chosen == from {
                        *chosen = to;
                    }
                }
                if let Some(i) = delete {
                    instances.remove(i);
                    *chosen = (*chosen).min(instances.len().saturating_sub(1));
                }
            });
        });
    data_commit(ui, edit)
}

/// The chosen instance's editor; returns the text to write. See
/// docs/features/instances.md#canvas.
pub(super) fn canvas_data(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    chosen: &mut HashMap<PathBuf, usize>,
) -> Option<String> {
    let chosen = chosen.entry(inspected.path.clone()).or_default();
    let mut ui = ui.new_child(
        UiBuilder::new()
            .id_salt("data_editor")
            .max_rect(rect.shrink(16.0))
            .layout(Layout::top_down(Align::Min)),
    );
    let ui = &mut ui;
    let edit = match &mut inspected.data {
        Some(Ok(edit)) => edit,
        Some(Err((e, text))) => {
            unreadable(ui, e, text);
            return None;
        }
        None => return None,
    };
    let schema = match &edit.schema {
        Ok(schema) => schema.clone(),
        Err(e) => {
            let e = format!("Schema {}: {e}", edit.edit.schema);
            let text = edit.text.clone();
            unreadable(ui, &e, &text);
            return data_commit(ui, edit);
        }
    };
    if edit.edit.instances.is_empty() {
        ui.weak("No instances: add one in the Inspector's Instances tab");
        return data_commit(ui, edit);
    }
    *chosen = (*chosen).min(edit.edit.instances.len() - 1);
    let i = *chosen;
    let problems = edit.edit.check(&schema);
    let mine: Vec<&DataProblem> = problems.iter().filter(|p| p.instance == i).collect();
    // Refs choose among this file's instances.
    let ids: Vec<(String, String)> = edit
        .edit
        .instances
        .iter()
        .map(|inst| (inst.ty.clone(), inst.id.clone()))
        .collect();
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        property_style(ui);
        instance_header(ui, edit, i, &mine);
        ui.add_space(12.0);
        let Some(si) = adata::struct_index(&schema, &edit.edit.instances[i].ty) else {
            return;
        };
        if edit.started.insert(i) {
            // Groups marked collapsed start folded.
            if let Body::Struct { groups, .. } = &schema.types[si].body {
                for g in groups.iter().filter(|g| g.collapsed) {
                    edit.folded.insert((i, g.name.clone()));
                }
            }
        }
        let DataEdit {
            edit: data,
            folded,
            open,
            picker,
            curves,
            curve_settings,
            gradients,
            ..
        } = edit;
        let mut cx = Cx {
            schema: &schema,
            ids: &ids,
            problems: &mine,
            open,
            instance: i,
            left: ui.cursor().left(),
            editors: ValueEditors {
                picker,
                curves,
                curve_settings,
                gradients,
                ti: i,
            },
        };
        let values = &mut data.instances[i].values;
        fields(ui, &mut cx, si, values, None, "", Some(folded));
    });
    data_commit(ui, edit)
}

/// An error and the file's text, read only.
fn unreadable(ui: &mut Ui, e: &str, text: &str) {
    ui.label(RichText::new(e).color(CHANNEL_R));
    ui.add_space(8.0);
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        ui.label(RichText::new(text).monospace().color(TEXT_WEAK));
    });
}

/// Type, id and the instance's own problems.
fn instance_header(ui: &mut Ui, edit: &mut DataEdit, i: usize, problems: &[&DataProblem]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let inst = &edit.edit.instances[i];
        ui.label(RichText::new(&inst.ty).monospace().color(TEXT_WEAK));
        let old = inst.id.clone();
        let ty = inst.ty.clone();
        if let Some(new) = commit_field(ui, ("instance_id", i), &old, 200.0, "Id") {
            let taken = edit
                .edit
                .instances
                .iter()
                .enumerate()
                .any(|(k, o)| k != i && o.ty == ty && o.id == new);
            if aschema::is_identifier(&new) && !taken {
                edit.edit.instances[i].id = new;
            }
        }
        let own: Vec<&DataProblem> = problems
            .iter()
            .copied()
            .filter(|p| p.field.is_none())
            .collect();
        issues(ui, "instance", &own);
    });
}

/// What the field rows share.
struct Cx<'a> {
    schema: &'a Schema,
    ids: &'a [(String, String)],
    problems: &'a [&'a DataProblem],
    open: &'a mut HashSet<(usize, String)>,
    instance: usize,
    /// Where labels start at the top level: indented labels are narrower, so values line up.
    left: f32,
    editors: ValueEditors<'a>,
}

/// The rows of struct `si`'s fields in `values` (what is set), inheriting from `outer` (the
/// enclosing default); `folded` groups at the instance's top level only. See
/// docs/features/instances.md#canvas.
fn fields(
    ui: &mut Ui,
    cx: &mut Cx,
    si: usize,
    values: &mut Map<String, Value>,
    outer: Option<&Map<String, Value>>,
    path: &str,
    folded: Option<&mut HashSet<(usize, String)>>,
) {
    let Body::Struct { fields, groups } = &cx.schema.types[si].body else {
        return;
    };
    // Sorted by order, ties in schema order.
    let mut sorted: Vec<&Field> = fields.iter().collect();
    sorted.sort_by(|a, b| {
        let order = |f: &Field| f.order.as_ref().and_then(|n| n.as_f64()).unwrap_or(0.0);
        order(a).total_cmp(&order(b))
    });
    let resolved = |values: &Map<String, Value>, name: &str| -> Option<Value> {
        let f = fields.iter().find(|f| f.name == name)?;
        Some(
            values
                .get(name)
                .cloned()
                .unwrap_or_else(|| inherited(cx.schema, f, outer)),
        )
    };
    let visible = |values: &Map<String, Value>, f: &Field| {
        (f.show_if.is_empty() || aschema::condition_holds(&f.show_if, |n| resolved(values, n)))
            && (f.hide_if.is_empty()
                || !aschema::condition_holds(&f.hide_if, |n| resolved(values, n)))
    };
    let Some(folded) = folded else {
        for f in sorted {
            if visible(values, f) {
                field_row(ui, cx, f, values, outer, path);
            }
        }
        return;
    };
    // Sections: ungrouped, then the struct's groups, then groups only fields name.
    let mut names: Vec<String> = groups.iter().map(|g| g.name.clone()).collect();
    for f in &sorted {
        if !f.group.is_empty() && !names.contains(&f.group) {
            names.push(f.group.clone());
        }
    }
    for f in sorted.iter().filter(|f| f.group.is_empty()) {
        if visible(values, f) {
            field_row(ui, cx, f, values, outer, path);
        }
    }
    for name in names {
        let key = (cx.instance, name.clone());
        let is_folded = folded.contains(&key);
        ui.add_space(6.0);
        let icon = if is_folded {
            Icon::ChevronRight
        } else {
            Icon::ChevronDown
        };
        let header = ui
            .horizontal(|ui| {
                icon.image(14.0, TEXT_WEAK).paint_at(
                    ui,
                    Rect::from_min_size(ui.cursor().min + vec2(0.0, 4.0), vec2(14.0, 14.0)),
                );
                ui.add_space(18.0);
                ui.label(RichText::new(&name).size(14.0).color(TEXT_WEAK));
            })
            .response
            .interact(Sense::click());
        let doc = groups
            .iter()
            .find(|g| g.name == name)
            .map(|g| g.doc.clone())
            .unwrap_or_default();
        let header = if doc.is_empty() {
            header
        } else {
            header.on_hover_text(doc)
        };
        if header.clicked() {
            if is_folded {
                folded.remove(&key);
            } else {
                folded.insert(key);
            }
        }
        if !is_folded {
            ui.indent(("group", &name), |ui| {
                for f in sorted.iter().filter(|f| f.group == name) {
                    if visible(values, f) {
                        field_row(ui, cx, f, values, outer, path);
                    }
                }
            });
        }
    }
}

/// One field's row (label, optional checkbox, editor, Reset, problems), then a struct's
/// fields or a list's items under it.
fn field_row(
    ui: &mut Ui,
    cx: &mut Cx,
    f: &Field,
    values: &mut Map<String, Value>,
    outer: Option<&Map<String, Value>>,
    path: &str,
) {
    let ty = parse_type(&f.ty).ok();
    let path = format!("{path}/{}", f.name);
    let from = inherited(cx.schema, f, outer);
    let set = values.contains_key(&f.name);
    let mut value = values.get(&f.name).cloned().unwrap_or_else(|| from.clone());
    let before = value.clone();
    let inner = struct_of(cx.schema, ty.as_ref()).map(|(si, _)| si);
    let inline = f.widget == "inline";
    let key = (cx.instance, path.clone());
    let stored = values.get(&f.name).and_then(Value::as_object).cloned();
    let mut reset = false;
    let mut nested_write = None;
    ui.push_id(&path, |ui| {
        ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            ui.spacing_mut().item_spacing.x = 6.0;
            let label = if f.label.is_empty() {
                &f.name
            } else {
                &f.label
            };
            let color = if set { TEXT } else { TEXT_WEAK };
            let indent = ui.cursor().left() - cx.left;
            let width = (LABEL_W - indent).max(60.0);
            let (rect, response) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
            ui.painter().with_clip_rect(rect).text(
                rect.left_center(),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(14.0),
                color,
            );
            let tip = if f.doc.is_empty() {
                format!("{}: {}", f.name, f.ty)
            } else {
                format!("{}: {}\n{}", f.name, f.ty, f.doc)
            };
            response.on_hover_text(tip);
            // The optional checkbox, or its room.
            if f.optional {
                let mut has = !value.is_null();
                if ui.checkbox(&mut has, "").changed() {
                    value = match (has, &f.default) {
                        (false, _) => Value::Null,
                        (true, Some(d)) if !d.is_null() => d.clone(),
                        (true, _) => ty
                            .as_ref()
                            .map_or(Value::Null, |t| aschema::implicit_value(cx.schema, t)),
                    };
                }
            } else {
                ui.add_space(18.0);
            }
            ui.add_enabled_ui(!f.readonly, |ui| {
                ui.allocate_ui_with_layout(
                    vec2(VALUE_W, 22.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(VALUE_W);
                        if value.is_null() && f.optional {
                            ui.label(RichText::new("none").color(TEXT_WEAK));
                        } else if inner.is_some() && value.is_object() {
                            struct_summary(ui, cx, f, stored.as_ref(), inline, &key);
                        } else {
                            data_value(ui, cx, f, ty.as_ref(), &mut value, &path);
                        }
                    },
                );
            });
            if set && icon_button(ui, Icon::RotateCcw, "Reset", !f.readonly).clicked() {
                reset = true;
            }
            let top = path.split('/').nth(1).unwrap_or_default();
            if path.matches('/').count() == 1 {
                let mine: Vec<&DataProblem> = cx
                    .problems
                    .iter()
                    .copied()
                    .filter(|p| p.field.as_deref() == Some(top))
                    .collect();
                issues(ui, "problems", &mine);
            }
        });
        // A struct's fields, unfolded or inline; a list's items.
        if let Some(si) = inner
            && value.is_object()
            && (inline || cx.open.contains(&key))
        {
            // Its fields inherit from this field's resolved default; what they set is stored.
            let mut nested = stored.clone().unwrap_or_default();
            let outer = from.as_object().cloned();
            ui.add_enabled_ui(!f.readonly, |ui| {
                ui.indent("fields", |ui| {
                    fields(ui, cx, si, &mut nested, outer.as_ref(), &path, None);
                });
            });
            if Some(&nested) != stored.as_ref() && (stored.is_some() || !nested.is_empty()) {
                nested_write = Some(nested);
            }
        } else if matches!(ty, Some(Ty::List(_))) && value.is_array() {
            ui.add_enabled_ui(!f.readonly, |ui| {
                let mut shown = Vec::new();
                nested_value(
                    ui,
                    &mut cx.editors,
                    &mut value,
                    cx.schema,
                    ty.as_ref(),
                    &path,
                    &mut shown,
                );
            });
        }
    });
    if reset {
        values.remove(&f.name);
    } else if let Some(nested) = nested_write {
        values.insert(f.name.clone(), Value::Object(nested));
    } else if !same(&value, &before) {
        values.insert(f.name.clone(), value);
    }
}

/// A struct value's `n of m set` and its fold button (none when inline).
fn struct_summary(
    ui: &mut Ui,
    cx: &mut Cx,
    f: &Field,
    stored: Option<&Map<String, Value>>,
    inline: bool,
    key: &(usize, String),
) {
    let ty = parse_type(&f.ty).ok();
    let (_, fields) = struct_of(cx.schema, ty.as_ref()).unwrap_or_default();
    if !inline {
        let open = cx.open.contains(key);
        let icon = if open {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        if icon_button(ui, icon, if open { "Fold" } else { "Unfold" }, true).clicked() {
            if open {
                cx.open.remove(key);
            } else {
                cx.open.insert(key.clone());
            }
        }
    }
    let set = fields
        .iter()
        .filter(|f| stored.is_some_and(|o| o.contains_key(&f.name)))
        .count();
    ui.label(RichText::new(format!("{set} of {} set", fields.len())).color(TEXT_WEAK));
}

/// A value's editor with the field's rules: toggles, ranges and steps, refs. See
/// docs/features/instances.md#editors.
fn data_value(ui: &mut Ui, cx: &mut Cx, f: &Field, ty: Option<&Ty>, value: &mut Value, path: &str) {
    let number = |n: &Option<serde_json::Number>| n.as_ref().and_then(|n| n.as_f64());
    let (min, max, step) = (number(&f.min), number(&f.max), number(&f.step));
    let enum_values = match ty {
        Some(Ty::Named(n)) => {
            cx.schema
                .type_index(n)
                .and_then(|i| match &cx.schema.types[i].body {
                    Body::Enum { values } => Some(values.clone()),
                    Body::Struct { .. } => None,
                })
        }
        _ => None,
    };
    match (ty, enum_values) {
        (_, Some(values)) if f.widget == "toggle" => {
            let current = value.as_str().unwrap_or_default().to_owned();
            ui.spacing_mut().item_spacing.x = 2.0;
            for v in values {
                let on = v == current;
                let text = RichText::new(&v).color(if on { BG_BAR } else { TEXT });
                if ui.add(egui::Button::new(text).selected(on)).clicked() {
                    *value = Value::String(v);
                }
            }
        }
        (Some(Ty::Prim(p)), _) if ty.is_some_and(Ty::is_integer) => {
            let (lo, hi) = aschema::int_range(p);
            let lo =
                (lo.max(i64::MIN.into()) as i64).max(min.map_or(i64::MIN, |m| m.ceil() as i64));
            let hi =
                (hi.min(i64::MAX.into()) as i64).min(max.map_or(i64::MAX, |m| m.floor() as i64));
            let mut n = value
                .as_i64()
                .unwrap_or_else(|| value.as_f64().unwrap_or(0.0) as i64);
            let field = egui::DragValue::new(&mut n)
                .range(lo..=hi)
                .speed(step.unwrap_or(1.0) / 4.0);
            if ui.add_sized(vec2(VALUE_W, 22.0), field).changed() {
                *value = n.into();
            }
        }
        (Some(ty), _) if ty.is_number() => {
            let mut n = value.as_f64().unwrap_or(0.0);
            let mut field = egui::DragValue::new(&mut n)
                .speed(step.unwrap_or(0.01))
                .max_decimals(3);
            if min.is_some() || max.is_some() {
                field = field.range(min.unwrap_or(f64::MIN)..=max.unwrap_or(f64::MAX));
            }
            if ui.add_sized(vec2(VALUE_W, 22.0), field).changed() {
                *value = aschema::json_number(n);
            }
        }
        (Some(Ty::Ref(target)), _) => {
            let target = target.to_string();
            let current = value.as_str().unwrap_or_default().to_owned();
            let shown = if current.is_empty() { "—" } else { &current };
            egui::ComboBox::from_id_salt("ref")
                .width(VALUE_W - 8.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(current.is_empty(), "—").clicked() {
                        *value = Value::String(String::new());
                    }
                    for (ty, id) in cx.ids {
                        if *ty == target && ui.selectable_label(*id == current, id).clicked() {
                            *value = Value::String(id.clone());
                        }
                    }
                });
        }
        _ => value_editor(ui, &mut cx.editors, value, cx.schema, ty, path),
    }
}
