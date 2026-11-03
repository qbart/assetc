//! The Config dialog: `assetc.toml` edited per target, saved keeping the file's comments and
//! layout; see docs/features/config.md.

use std::fs;

use super::*;
use crate::config::{self as project_config, FILE_NAME};
use crate::texture;
use egui::Color32;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, TableLike, Value};

/// Every setting's default, as `assetc.toml` would spell it; see docs/spec/assetc.toml.md.
const DEFAULTS: &str = r#"
input = "assets"
output = "build"

[textures]
linear = []
no_mipmaps = []
volume_luts = []

[meshes]
meshlet_max_vertices = 64
meshlet_max_triangles = 124
meshlet_cone_weight = 0.25
lod_ratios = [0.5, 0.25]
lod_error = 0.05
lod_error_step = 0.05
lod_attempts = 3
lod_sloppy_fallback = true
lod_uv_weight = 1.0
lod_normal_weight = 0.5
lod_permissive = true
lod_lock_border = false
lod_regularize = false
lod_prune = false

[pack]
file = "assetc.pak"
alignment = 16
include = ["**"]
exclude = []
"#;

#[derive(Clone, Copy, PartialEq)]
enum Section {
    General,
    Textures,
    Meshes,
    Locators,
    Pack,
}

impl Section {
    const ALL: [Self; 5] = [
        Self::General,
        Self::Textures,
        Self::Meshes,
        Self::Locators,
        Self::Pack,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Textures => "Textures",
            Self::Meshes => "Meshes",
            Self::Locators => "Locators",
            Self::Pack => "Pack",
        }
    }
}

/// How a setting is edited.
#[derive(Clone, Copy)]
enum Edit {
    Text,
    Globs,
    Int { min: i64, max: i64, step: i64 },
    Float { min: f64, max: f64 },
    Bool,
    Floats,
    Alignment,
}

/// One setting: its table (`None` at the top level), key, editor and meaning.
struct Field {
    table: Option<&'static str>,
    key: &'static str,
    edit: Edit,
    help: &'static str,
}

const fn field(
    table: Option<&'static str>,
    key: &'static str,
    edit: Edit,
    help: &'static str,
) -> Field {
    Field {
        table,
        key,
        edit,
        help,
    }
}

const GENERAL: [Field; 2] = [
    field(None, "input", Edit::Text, "Directory holding source assets"),
    field(
        None,
        "output",
        Edit::Text,
        "Directory compiled assets are written to",
    ),
];

const TEXTURES: [Field; 3] = [
    field(
        Some("textures"),
        "linear",
        Edit::Globs,
        "Globs of textures holding linear data, not sRGB color",
    ),
    field(
        Some("textures"),
        "no_mipmaps",
        Edit::Globs,
        "Globs of textures written without mipmaps",
    ),
    field(
        Some("textures"),
        "volume_luts",
        Edit::Globs,
        "Globs of color LUTs compiled as 3D textures instead of 2D strips",
    ),
];

const MESHES: [Field; 14] = [
    field(
        Some("meshes"),
        "meshlet_max_vertices",
        Edit::Int {
            min: 3,
            max: 256,
            step: 1,
        },
        "Most vertices per meshlet",
    ),
    field(
        Some("meshes"),
        "meshlet_max_triangles",
        Edit::Int {
            min: 4,
            max: 512,
            step: 4,
        },
        "Most triangles per meshlet, a multiple of 4",
    ),
    field(
        Some("meshes"),
        "meshlet_cone_weight",
        Edit::Float { min: 0.0, max: 1.0 },
        "How much meshlet building favours tight normal cones over compact meshlets",
    ),
    field(
        Some("meshes"),
        "lod_ratios",
        Edit::Floats,
        "Reduced levels of detail, as decreasing fractions of the full triangle count",
    ),
    field(
        Some("meshes"),
        "lod_error",
        Edit::Float { min: 0.0, max: 1.0 },
        "First error limit of simplification, relative to the mesh's size",
    ),
    field(
        Some("meshes"),
        "lod_error_step",
        Edit::Float { min: 0.0, max: 1.0 },
        "How much the limit rises per retry",
    ),
    field(
        Some("meshes"),
        "lod_attempts",
        Edit::Int {
            min: 0,
            max: 10,
            step: 1,
        },
        "Retries with a raised limit while a level makes no progress",
    ),
    field(
        Some("meshes"),
        "lod_sloppy_fallback",
        Edit::Bool,
        "After the retries, fall back to sloppy simplification",
    ),
    field(
        Some("meshes"),
        "lod_uv_weight",
        Edit::Float {
            min: 0.0,
            max: 100.0,
        },
        "Weight of UV error in simplification",
    ),
    field(
        Some("meshes"),
        "lod_normal_weight",
        Edit::Float {
            min: 0.0,
            max: 100.0,
        },
        "Weight of normal error in simplification",
    ),
    field(
        Some("meshes"),
        "lod_permissive",
        Edit::Bool,
        "Allow collapses across attribute seams",
    ),
    field(
        Some("meshes"),
        "lod_lock_border",
        Edit::Bool,
        "Keep vertices on the mesh's open borders in place",
    ),
    field(
        Some("meshes"),
        "lod_regularize",
        Edit::Bool,
        "Prefer evenly sized triangles, at some cost to accuracy",
    ),
    field(
        Some("meshes"),
        "lod_prune",
        Edit::Bool,
        "Remove small disconnected parts while simplifying",
    ),
];

const PACK: [Field; 4] = [
    field(
        Some("pack"),
        "file",
        Edit::Text,
        "Path of the pack, relative to output, ending in .pak",
    ),
    field(
        Some("pack"),
        "alignment",
        Edit::Alignment,
        "Byte alignment of every packed file",
    ),
    field(
        Some("pack"),
        "include",
        Edit::Globs,
        "Globs of the output files to pack",
    ),
    field(
        Some("pack"),
        "exclude",
        Edit::Globs,
        "Globs of output files left out, even when included",
    ),
];

/// The open Config dialog.
pub(super) struct ConfigDialog {
    path: PathBuf,
    root: PathBuf,
    /// The edited copy; `Err` when the file did not load.
    doc: Result<DocumentMut, String>,
    defaults: DocumentMut,
    /// `None` is the default target.
    target: Option<String>,
    section: Section,
    /// The first problem of the edited copy.
    problem: Option<String>,
    /// A new target's name being typed.
    naming: Option<String>,
    confirm_remove: bool,
    /// Input-relative glTF models, for locator presets.
    models: Vec<String>,
}

/// What the dialog did this frame.
pub(super) enum DialogEnd {
    Open,
    Close,
    Saved(Project),
}

impl ConfigDialog {
    pub(super) fn open(project: &Project) -> Self {
        let root = project.root.clone().unwrap_or_default();
        let path = root.join(FILE_NAME);
        let doc = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| text.parse::<DocumentMut>().map_err(|e| e.to_string()));
        let models = project
            .input_dir()
            .map(|input| {
                let mut models: Vec<String> = build::files_under(&input)
                    .iter()
                    .filter(|f| crate::model::is_model(f))
                    .filter_map(|f| build::relative(&input, f).ok())
                    .collect();
                models.sort();
                models
            })
            .unwrap_or_default();
        let mut dialog = Self {
            path,
            root,
            doc,
            defaults: DEFAULTS.parse().expect("defaults parse"),
            target: None,
            section: Section::General,
            problem: None,
            naming: None,
            confirm_remove: false,
            models,
        };
        dialog.check();
        dialog
    }

    fn check(&mut self) {
        self.problem = match &self.doc {
            Ok(doc) => check(&doc.to_string()).err(),
            Err(_) => None,
        };
    }
}

/// The text as loading and building check it; see docs/features/config.md#saving.
fn check(text: &str) -> Result<project_config::Config, String> {
    let config = project_config::parse(text).map_err(|e| e.to_string())?;
    let mut all = vec![(String::new(), config.clone())];
    for name in config.targets.keys() {
        all.push((
            format!("targets.{name}: "),
            config.target(name).map_err(|e| e.to_string())?,
        ));
    }
    for (at, c) in all {
        texture::Settings::new(&c.textures)
            .map_err(|e| format!("{at}{FILE_NAME} [textures]: {e}"))?;
        crate::amesh::MeshSettings::new(&c.meshes)
            .map_err(|e| format!("{at}{FILE_NAME} [meshes]: {e}"))?;
        crate::pack::Settings::new(&c.pack).map_err(|e| format!("{at}{e}"))?;
    }
    Ok(config)
}

/// The settings table of a target (`None`: the top level).
fn scope<'a>(doc: &'a DocumentMut, target: Option<&str>) -> Option<&'a dyn TableLike> {
    match target {
        None => Some(doc.as_table()),
        Some(name) => doc
            .get("targets")?
            .as_table_like()?
            .get(name)?
            .as_table_like(),
    }
}

/// `key` of `table` (`None`: the scope itself) in `scope`.
fn lookup<'a>(scope: &'a dyn TableLike, table: Option<&str>, key: &str) -> Option<&'a Item> {
    match table {
        None => scope.get(key),
        Some(t) => scope.get(t)?.as_table_like()?.get(key),
    }
    .filter(|item| !item.is_none())
}

/// The table at `key` of `parent`, made a table when it is missing or something else.
fn table_mut<'a>(
    parent: &'a mut dyn TableLike,
    key: &str,
    implicit: bool,
) -> &'a mut dyn TableLike {
    if parent.get(key).and_then(Item::as_table_like).is_none() {
        let mut table = Table::new();
        table.set_implicit(implicit);
        parent.insert(key, Item::Table(table));
    }
    parent
        .get_mut(key)
        .and_then(Item::as_table_like_mut)
        .expect("table just made")
}

fn scope_mut<'a>(doc: &'a mut DocumentMut, target: Option<&str>) -> &'a mut dyn TableLike {
    match target {
        None => doc.as_table_mut(),
        Some(name) => table_mut(table_mut(doc.as_table_mut(), "targets", true), name, false),
    }
}

/// Writes `item` at `table.key` of the scope.
fn set(doc: &mut DocumentMut, target: Option<&str>, table: Option<&str>, key: &str, item: Item) {
    let scope = scope_mut(doc, target);
    let parent = match table {
        None => scope,
        Some(t) => table_mut(scope, t, false),
    };
    parent.insert(key, item);
}

/// Removes `table.key` of the scope; a target's table left empty goes too.
fn reset(doc: &mut DocumentMut, target: Option<&str>, table: Option<&str>, key: &str) {
    let scope = scope_mut(doc, target);
    match table {
        None => {
            scope.remove(key);
        }
        Some(t) => {
            if let Some(parent) = scope.get_mut(t).and_then(Item::as_table_like_mut) {
                parent.remove(key);
                if target.is_some() && parent.is_empty() {
                    scope.remove(t);
                }
            }
        }
    }
}

fn strings(item: Option<&Item>) -> Vec<String> {
    item.and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn number(item: Option<&Item>) -> f64 {
    item.and_then(|i| i.as_float().or_else(|| i.as_integer().map(|n| n as f64)))
        .unwrap_or_default()
}

fn numbers(item: Option<&Item>) -> Vec<f64> {
    item.and_then(Item::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
                .collect()
        })
        .unwrap_or_default()
}

fn string_array(items: &[String]) -> Item {
    toml_edit::value(items.iter().map(String::as_str).collect::<Array>())
}

fn float_array(items: &[f64]) -> Item {
    toml_edit::value(items.iter().copied().collect::<Array>())
}

/// The tables of an array of tables, written either way.
fn tables(item: Option<&Item>) -> Vec<Table> {
    match item {
        Some(Item::ArrayOfTables(a)) => a.iter().cloned().collect(),
        Some(Item::Value(Value::Array(a))) => a
            .iter()
            .filter_map(|v| v.as_inline_table().map(|t| t.clone().into_table()))
            .collect(),
        _ => Vec::new(),
    }
}

fn tables_item(tables: Vec<Table>) -> Item {
    let mut array = ArrayOfTables::new();
    for table in tables {
        array.push(table);
    }
    Item::ArrayOfTables(array)
}

/// What a row asks for.
enum Change {
    Set(Item),
    Reset,
}

/// The dialog; see docs/features/config.md.
pub(super) fn config_dialog(ctx: &egui::Context, dialog: &mut ConfigDialog) -> DialogEnd {
    let frame = Frame::window(&ctx.global_style())
        .fill(BG_PANEL)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(8)
        .inner_margin(24);
    let screen = ctx.content_rect().size();
    let size = vec2((screen.x - 80.0).min(880.0), (screen.y - 80.0).min(600.0));
    let mut end = DialogEnd::Open;
    egui::Modal::new(egui::Id::new("config_dialog"))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(size.x);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Config").size(20.0).color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(dialog.path.display().to_string())
                            .size(13.0)
                            .color(TEXT_WEAK),
                    );
                });
            });
            ui.add_space(12.0);
            let doc = match &mut dialog.doc {
                Ok(doc) => doc,
                Err(e) => {
                    ui.label(RichText::new(e.as_str()).size(14.0).color(CHANNEL_R));
                    ui.add_space(16.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if dialog_button(ui, "Close", false, true).clicked() {
                            end = DialogEnd::Close;
                        }
                    });
                    return;
                }
            };
            let mut changed = false;
            target_bar(
                ui,
                doc,
                &mut dialog.target,
                &mut dialog.naming,
                &mut dialog.confirm_remove,
                &mut changed,
            );
            ui.add_space(12.0);
            let body = vec2(size.x, size.y - 150.0);
            ui.allocate_ui_with_layout(body, Layout::left_to_right(Align::Min), |ui| {
                ui.set_min_size(body);
                ui.vertical(|ui| {
                    ui.set_width(140.0);
                    for section in Section::ALL {
                        let selected = dialog.section == section;
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(140.0, 30.0), Sense::click());
                        let fill = if selected {
                            ACCENT.gamma_multiply(0.25)
                        } else if response.hovered() {
                            BG_HEADER
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, 4, fill);
                        ui.painter().text(
                            pos2(rect.left() + 10.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            section.name(),
                            FontId::proportional(14.0),
                            TEXT,
                        );
                        if response.clicked() {
                            dialog.section = section;
                        }
                    }
                });
                ui.add_space(16.0);
                egui::ScrollArea::vertical()
                    .id_salt("config_fields")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        let target = dialog.target.as_deref();
                        let defaults = &dialog.defaults;
                        match dialog.section {
                            Section::General => {
                                fields(ui, doc, defaults, target, &GENERAL, &mut changed)
                            }
                            Section::Textures => {
                                fields(ui, doc, defaults, target, &TEXTURES, &mut changed)
                            }
                            Section::Meshes => {
                                fields(ui, doc, defaults, target, &MESHES, &mut changed);
                                ui.add_space(12.0);
                                overrides(ui, doc, defaults, target, &mut changed);
                            }
                            Section::Locators => {
                                presets(ui, doc, target, &dialog.models, &mut changed)
                            }
                            Section::Pack => fields(ui, doc, defaults, target, &PACK, &mut changed),
                        }
                    });
            });
            ui.add_space(12.0);
            let save_keys = ui.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::S,
                ))
            });
            let text = doc.to_string();
            ui.horizontal(|ui| {
                if let Some(problem) = &dialog.problem {
                    ui.label(RichText::new(problem.as_str()).size(13.0).color(CHANNEL_R));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let ok = dialog.problem.is_none();
                    if (dialog_button(ui, "Save", true, ok).clicked() || save_keys) && ok {
                        end = match fs::write(&dialog.path, &text)
                            .map_err(|e| e.to_string())
                            .and_then(|()| {
                                project_config::load(&dialog.root).map_err(|e| e.to_string())
                            }) {
                            Ok(project) => DialogEnd::Saved(project),
                            Err(e) => {
                                dialog.problem = Some(e);
                                DialogEnd::Open
                            }
                        };
                    }
                    ui.add_space(8.0);
                    if dialog_button(ui, "Cancel", false, true).clicked() {
                        end = DialogEnd::Close;
                    }
                });
            });
            if changed {
                dialog.check();
            }
        });
    // Escape cancels the new target's name first; a click outside does nothing.
    if matches!(end, DialogEnd::Open) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        if dialog.naming.take().is_none() && !std::mem::take(&mut dialog.confirm_remove) {
            end = DialogEnd::Close;
        }
    }
    end
}

fn dialog_button(ui: &mut Ui, label: &str, primary: bool, enabled: bool) -> Response {
    let (fill, color) = if primary {
        (ACCENT, BG_BAR)
    } else {
        (BG_HEADER, TEXT)
    };
    let button = egui::Button::new(RichText::new(label).size(14.0).color(color))
        .fill(fill)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(6)
        .min_size(vec2(96.0, 34.0));
    ui.add_enabled(enabled, button)
}

/// The Target dropdown, + Target and remove.
fn target_bar(
    ui: &mut Ui,
    doc: &mut DocumentMut,
    target: &mut Option<String>,
    naming: &mut Option<String>,
    confirm_remove: &mut bool,
    changed: &mut bool,
) {
    let names: Vec<String> = doc
        .get("targets")
        .and_then(Item::as_table_like)
        .map(|t| t.iter().map(|(k, _)| k.to_owned()).collect())
        .unwrap_or_default();
    if target.as_ref().is_some_and(|t| !names.contains(t)) {
        *target = None;
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new("Target").size(14.0).color(TEXT_WEAK));
        egui::ComboBox::from_id_salt("config_target")
            .width(200.0)
            .selected_text(target.as_deref().unwrap_or("Default"))
            .show_ui(ui, |ui| {
                ui.selectable_value(target, None, "Default");
                for name in &names {
                    ui.selectable_value(target, Some(name.clone()), name);
                }
            });
        match naming {
            Some(name) => {
                let response = ui.add(
                    egui::TextEdit::singleline(name)
                        .hint_text("Target name")
                        .desired_width(160.0),
                );
                response.request_focus();
                let valid = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    && !names.contains(name);
                if !name.is_empty() && !valid {
                    ui.label(RichText::new("Letters, digits, _ and -; not taken").color(CHANNEL_R));
                }
                if valid && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let name = std::mem::take(name);
                    scope_mut(doc, Some(&name));
                    *target = Some(name);
                    *naming = None;
                    *changed = true;
                }
            }
            None => {
                if ui.button("+ Target").clicked() {
                    *naming = Some(String::new());
                }
            }
        }
        if let Some(name) = target.clone() {
            if *confirm_remove {
                ui.label(
                    RichText::new(format!("Remove target {name:?}?"))
                        .size(14.0)
                        .color(TEXT),
                );
                if ui.button("Remove").clicked() {
                    if let Some(targets) = doc.get_mut("targets").and_then(Item::as_table_like_mut)
                    {
                        targets.remove(&name);
                        if targets.is_empty() {
                            doc.remove("targets");
                        }
                    }
                    *target = None;
                    *confirm_remove = false;
                    *changed = true;
                }
                if ui.button("Cancel").clicked() {
                    *confirm_remove = false;
                }
            } else if icon_button(ui, Icon::Trash2, "Remove target", true).clicked() {
                *confirm_remove = true;
            }
        }
    });
}

/// The value in effect of a field and whether the scope sets it.
fn effective<'a>(
    doc: &'a DocumentMut,
    defaults: &'a DocumentMut,
    target: Option<&str>,
    f: &Field,
) -> (Option<Item>, bool) {
    let here = scope(doc, target).and_then(|s| lookup(s, f.table, f.key));
    if let Some(item) = here {
        return (Some(item.clone()), true);
    }
    let top = lookup(doc.as_table(), f.table, f.key)
        .or_else(|| lookup(defaults.as_table(), f.table, f.key))
        .cloned();
    // A target writes to `<output>-<name>` unless it says otherwise.
    let top = match (target, f.key, f.table) {
        (Some(name), "output", None) => {
            top.and_then(|i| i.as_str().map(|o| toml_edit::value(format!("{o}-{name}"))))
        }
        _ => top,
    };
    (top, false)
}

fn fields(
    ui: &mut Ui,
    doc: &mut DocumentMut,
    defaults: &DocumentMut,
    target: Option<&str>,
    list: &[Field],
    changed: &mut bool,
) {
    for f in list {
        let (item, set_here) = effective(doc, defaults, target, f);
        let shared = target.is_some() && f.key == "input";
        let change = ui
            .horizontal(|ui| {
                ui.add_enabled_ui(!shared, |ui| {
                    field_row(ui, f, item.as_ref(), set_here, shared, true)
                })
                .inner
            })
            .inner;
        match change {
            Some(Change::Set(item)) => {
                set(doc, target, f.table, f.key, item);
                *changed = true;
            }
            Some(Change::Reset) => {
                reset(doc, target, f.table, f.key);
                *changed = true;
            }
            None => {}
        }
        ui.add_space(4.0);
    }
}

/// A setting's name, editor and reset (`resettable`) or remove button.
fn field_row(
    ui: &mut Ui,
    f: &Field,
    item: Option<&Item>,
    set_here: bool,
    shared: bool,
    resettable: bool,
) -> Option<Change> {
    let color = if set_here { TEXT } else { TEXT_WEAK };
    let (label, _) = ui.allocate_exact_size(vec2(180.0, 24.0), Sense::hover());
    ui.painter().text(
        pos2(label.left(), label.center().y),
        Align2::LEFT_CENTER,
        f.key,
        FontId::proportional(14.0),
        color,
    );
    let help = if shared {
        "Shared by every target"
    } else {
        f.help
    };
    ui.interact(label, ui.id().with(("help", f.key)), Sense::hover())
        .on_hover_text(help);
    let mut change = editor(ui, f, item, color).map(Change::Set);
    if set_here && !shared {
        let (icon, tip) = if resettable {
            (Icon::RotateCcw, "Reset")
        } else {
            (Icon::X, "Remove")
        };
        if icon_button(ui, icon, tip, true).clicked() {
            change = Some(Change::Reset);
        }
    }
    change
}

/// The editor of a setting's value; the new value when edited.
fn editor(ui: &mut Ui, f: &Field, item: Option<&Item>, color: Color32) -> Option<Item> {
    let id = ui.id().with(("config", f.table, f.key));
    match f.edit {
        Edit::Text => {
            let mut text = item.and_then(Item::as_str).unwrap_or_default().to_owned();
            let response = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(320.0)
                    .text_color(color),
            );
            response.changed().then(|| toml_edit::value(text))
        }
        Edit::Globs => {
            let mut list = strings(item);
            list_editor(ui, id, &mut list, |ui, s| {
                ui.add(
                    egui::TextEdit::singleline(s)
                        .desired_width(280.0)
                        .text_color(color),
                )
                .changed()
            })
            .then(|| string_array(&list))
        }
        Edit::Floats => {
            let mut list = numbers(item);
            list_editor(ui, id, &mut list, |ui, v| {
                ui.add(egui::DragValue::new(v).range(0.0..=1.0).speed(0.01))
                    .changed()
            })
            .then(|| float_array(&list))
        }
        Edit::Int { min, max, step } => {
            let mut n = number(item) as i64;
            let response = ui.add(
                egui::DragValue::new(&mut n)
                    .range(min..=max)
                    .speed(step as f64 * 0.25),
            );
            response.changed().then(|| {
                let n = (n / step * step).clamp(min, max);
                toml_edit::value(n)
            })
        }
        Edit::Float { min, max } => {
            let mut v = number(item);
            let response = ui.add(
                egui::DragValue::new(&mut v)
                    .range(min..=max)
                    .speed((max - min) / 400.0)
                    .max_decimals(3),
            );
            response.changed().then(|| toml_edit::value(v))
        }
        Edit::Bool => {
            let mut on = item.and_then(Item::as_bool).unwrap_or_default();
            ui.checkbox(&mut on, "")
                .changed()
                .then(|| toml_edit::value(on))
        }
        Edit::Alignment => {
            let mut n = number(item) as i64;
            let before = n;
            egui::ComboBox::from_id_salt(id)
                .width(120.0)
                .selected_text(inspect::format_size(n as u64))
                .show_ui(ui, |ui| {
                    for shift in 4..=16 {
                        let v = 1i64 << shift;
                        ui.selectable_value(&mut n, v, inspect::format_size(v as u64));
                    }
                });
            (n != before).then(|| toml_edit::value(n))
        }
    }
}

/// A column of `item` editors with `x` buttons, then `+`; whether anything changed.
fn list_editor<T: Default>(
    ui: &mut Ui,
    id: egui::Id,
    list: &mut Vec<T>,
    mut item: impl FnMut(&mut Ui, &mut T) -> bool,
) -> bool {
    let mut changed = false;
    ui.vertical(|ui| {
        let mut remove = None;
        for (i, value) in list.iter_mut().enumerate() {
            ui.push_id(id.with(i), |ui| {
                ui.horizontal(|ui| {
                    changed |= item(ui, value);
                    if icon_button(ui, Icon::X, "Remove", true).clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            list.remove(i);
            changed = true;
        }
        if icon_button(ui, Icon::Plus, "Add", true).clicked() {
            list.push(T::default());
            changed = true;
        }
    });
    changed
}

/// `[[meshes.overrides]]`: a card per entry; see docs/features/config.md#settings.
fn overrides(
    ui: &mut Ui,
    doc: &mut DocumentMut,
    defaults: &DocumentMut,
    target: Option<&str>,
    changed: &mut bool,
) {
    let here = scope(doc, target).and_then(|s| lookup(s, Some("meshes"), "overrides"));
    let set_here = here.is_some();
    let mut list = tables(here.or_else(|| lookup(doc.as_table(), Some("meshes"), "overrides")));
    let color = if set_here { TEXT } else { TEXT_WEAK };
    let mut edited = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("overrides").size(14.0).color(color))
            .on_hover_text("Per-model values; the last matching entry that sets a value wins");
        if set_here && icon_button(ui, Icon::RotateCcw, "Reset", true).clicked() {
            reset(doc, target, Some("meshes"), "overrides");
            *changed = true;
        }
    });
    if !*changed {
        let mut remove = None;
        for (i, entry) in list.iter_mut().enumerate() {
            ui.push_id(("override", i), |ui| {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Override {}", i + 1)).color(color));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if icon_button(ui, Icon::X, "Remove override", true).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    let globs = field(
                        None,
                        "match",
                        Edit::Globs,
                        "Globs of the models it applies to",
                    );
                    let mut entry_changed = false;
                    ui.horizontal(|ui| {
                        if let Some(Change::Set(item)) =
                            field_row(ui, &globs, entry.get("match"), set_here, false, false)
                        {
                            entry.insert("match", item);
                            entry_changed = true;
                        }
                    });
                    let mut drop = None;
                    let present: Vec<&Field> = MESHES
                        .iter()
                        .filter(|f| entry.contains_key(f.key))
                        .collect();
                    for f in present {
                        ui.horizontal(|ui| {
                            match field_row(ui, f, entry.get(f.key), set_here, false, false) {
                                Some(Change::Set(item)) => {
                                    entry.insert(f.key, item);
                                    entry_changed = true;
                                }
                                Some(Change::Reset) => drop = Some(f.key),
                                None => {}
                            }
                        });
                    }
                    if let Some(key) = drop {
                        entry.remove(key);
                        entry_changed = true;
                    }
                    let missing: Vec<&Field> = MESHES
                        .iter()
                        .filter(|f| !entry.contains_key(f.key))
                        .collect();
                    if !missing.is_empty() {
                        ui.menu_button("+ Setting", |ui| {
                            for f in missing {
                                if ui.button(f.key).clicked() {
                                    let (value, _) = effective(doc, defaults, target, f);
                                    if let Some(value) = value {
                                        entry.insert(f.key, value);
                                        entry_changed = true;
                                    }
                                    ui.close();
                                }
                            }
                        });
                    }
                    edited |= entry_changed;
                });
            });
        }
        if let Some(i) = remove {
            list.remove(i);
            edited = true;
        }
        if ui.button("+ Override").clicked() {
            let mut entry = Table::new();
            entry.insert("match", string_array(&[]));
            list.push(entry);
            edited = true;
        }
    }
    if edited {
        set(doc, target, Some("meshes"), "overrides", tables_item(list));
        *changed = true;
    }
}

/// `[[locators.presets]]`: a card per entry; see docs/features/config.md#settings.
fn presets(
    ui: &mut Ui,
    doc: &mut DocumentMut,
    target: Option<&str>,
    models: &[String],
    changed: &mut bool,
) {
    let here = scope(doc, target).and_then(|s| lookup(s, Some("locators"), "presets"));
    let set_here = here.is_some();
    let mut list = tables(here.or_else(|| lookup(doc.as_table(), Some("locators"), "presets")));
    let color = if set_here { TEXT } else { TEXT_WEAK };
    let mut edited = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("presets").size(14.0).color(color))
            .on_hover_text("Locators of one model shared with others");
        if set_here && icon_button(ui, Icon::RotateCcw, "Reset", true).clicked() {
            reset(doc, target, Some("locators"), "presets");
            *changed = true;
        }
    });
    if *changed {
        return;
    }
    let mut remove = None;
    for (i, entry) in list.iter_mut().enumerate() {
        ui.push_id(("preset", i), |ui| {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Preset {}", i + 1)).color(color));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, Icon::X, "Remove preset", true).clicked() {
                            remove = Some(i);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let (label, _) = ui.allocate_exact_size(vec2(180.0, 24.0), Sense::hover());
                    ui.painter().text(
                        pos2(label.left(), label.center().y),
                        Align2::LEFT_CENTER,
                        "from",
                        FontId::proportional(14.0),
                        color,
                    );
                    let mut from = entry
                        .get("from")
                        .and_then(Item::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let before = from.clone();
                    egui::ComboBox::from_id_salt("from")
                        .width(320.0)
                        .selected_text(if from.is_empty() {
                            "Choose a model"
                        } else {
                            &from
                        })
                        .show_ui(ui, |ui| {
                            for model in models {
                                ui.selectable_value(&mut from, model.clone(), model);
                            }
                        });
                    if from != before {
                        entry.insert("from", toml_edit::value(from));
                        edited = true;
                    }
                });
                let globs = field(
                    None,
                    "match",
                    Edit::Globs,
                    "Globs of the models it applies to; empty: every model",
                );
                ui.horizontal(|ui| {
                    if let Some(Change::Set(item)) =
                        field_row(ui, &globs, entry.get("match"), set_here, false, false)
                    {
                        // An empty list is no `match`: every model.
                        if strings(Some(&item)).is_empty() {
                            entry.remove("match");
                        } else {
                            entry.insert("match", item);
                        }
                        edited = true;
                    }
                });
            });
        });
    }
    if let Some(i) = remove {
        list.remove(i);
        edited = true;
    }
    if ui.button("+ Preset").clicked() {
        let mut entry = Table::new();
        entry.insert(
            "from",
            toml_edit::value(models.first().cloned().unwrap_or_default()),
        );
        list.push(entry);
        edited = true;
    }
    if edited {
        set(doc, target, Some("locators"), "presets", tables_item(list));
        *changed = true;
    }
}

fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(BG_HEADER)
        .stroke(Stroke::new(1.0, STROKE))
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
    ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_configs_defaults() {
        let config = project_config::parse(DEFAULTS).unwrap();
        let default = project_config::Config::default();
        assert_eq!(config.input, default.input);
        assert_eq!(config.output, default.output);
        assert_eq!(config.textures, default.textures);
        assert_eq!(config.meshes, default.meshes);
        assert_eq!(config.pack, default.pack);
    }

    #[test]
    fn edits_keep_comments_and_targets_inherit() {
        let text = "# project\ninput = \"src\" # mine\n\n[textures]\nlinear = [\"a/**\"]\n";
        let mut doc: DocumentMut = text.parse().unwrap();
        let defaults: DocumentMut = DEFAULTS.parse().unwrap();
        set(
            &mut doc,
            None,
            Some("pack"),
            "alignment",
            toml_edit::value(4096),
        );
        set(
            &mut doc,
            Some("mobile"),
            Some("textures"),
            "no_mipmaps",
            string_array(&["ui/**".into()]),
        );
        let written = doc.to_string();
        assert!(written.starts_with("# project\ninput = \"src\" # mine\n"));
        let config = check(&written).unwrap();
        assert_eq!(config.pack.alignment, 4096);
        let mobile = config.target("mobile").unwrap();
        assert_eq!(mobile.textures.no_mipmaps, ["ui/**"]);
        assert_eq!(mobile.textures.linear, ["a/**"]);

        let output = &GENERAL[1];
        let (item, set_here) = effective(&doc, &defaults, Some("mobile"), output);
        assert_eq!(item.unwrap().as_str(), Some("build-mobile"));
        assert!(!set_here);
        let (item, set_here) = effective(&doc, &defaults, Some("mobile"), &TEXTURES[1]);
        assert!(set_here);
        assert_eq!(strings(item.as_ref()), ["ui/**"]);

        // Resetting the target's only key drops its emptied table.
        reset(&mut doc, Some("mobile"), Some("textures"), "no_mipmaps");
        assert!(!doc.to_string().contains("targets.mobile.textures"));
        assert!(
            check(&doc.to_string())
                .unwrap()
                .targets
                .contains_key("mobile")
        );
    }

    #[test]
    fn problems_name_the_setting() {
        assert!(
            check("[pack]\nalignment = 3\n")
                .unwrap_err()
                .contains("alignment 3")
        );
        assert!(
            check("[meshes]\nmeshlet_max_triangles = 6\n")
                .unwrap_err()
                .contains("[meshes]")
        );
        assert!(
            check("[targets.m.pack]\nfile = \"x.zip\"\n")
                .unwrap_err()
                .starts_with("targets.m: ")
        );
    }
}
