//! The Structs tab: the structs attached to an asset and an owner's values; see
//! docs/features/structs.md#structs-tab.

use serde_json::{Map, Value};

use super::data::{FieldsState, issues, struct_fields};
use super::graph::Chosen;
use super::*;
use crate::adata::{self, DataProblem};
use crate::structs::{self, Attachment, Problem, Schemas, Values};
use crate::theme::CHANNEL_R;

/// The tab's session state of one file.
#[derive(Default)]
pub(super) struct StructsView {
    fields: FieldsState,
    /// The owner whose values are edited, for gradient files and palettes: `0` the file, then
    /// each item from 1.
    owner: usize,
}

/// What the tab reads once per inspection: the schemas of the attachments with their
/// projects, every schema of the input with its structs, and the input's files.
pub(super) struct StructsCache {
    key: Vec<String>,
    schemas: Schemas,
    /// By path relative to the asset's folder: its structs, or why it can't be read.
    available: Vec<(String, Result<Vec<String>, String>)>,
    assets: Vec<String>,
}

/// What the tab writes: the asset's own file, its `atlas.toml`, or a model's sidecar.
pub(super) enum StructsSave {
    File(String),
    Atlas(String),
    Sidecar(Box<Amod>),
}

/// The asset being edited, from whichever editor holds it.
enum Host<'a> {
    Graph(&'a mut graph::GraphEdit),
    Model(&'a mut MeshEdit),
    Material(&'a mut MaterialEdit),
    Atlas(&'a mut (crate::atlas::AtlasConfig, crate::atlas::AtlasConfig)),
    Gradient(&'a mut GradientEdit),
    Palette(&'a mut PaletteEdit),
}

impl Host<'_> {
    /// Each scope's title and what its owners are called.
    fn scope_names(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Graph(g) if g.tree => &[("Tree structs", "tree"), ("Node structs", "node")],
            Self::Graph(_) => &[
                ("Graph structs", "graph"),
                ("Node structs", "node"),
                ("Edge structs", "edge"),
            ],
            Self::Model(_) => &[("Model structs", "model")],
            Self::Material(_) => &[("Material structs", "material")],
            Self::Atlas(_) => &[("Atlas structs", "atlas")],
            Self::Gradient(_) => &[("File structs", "file"), ("Gradient structs", "gradient")],
            Self::Palette(_) => &[("File structs", "file"), ("Color structs", "color")],
        }
    }

    fn attachments(&mut self, scope: usize) -> &mut Vec<Attachment> {
        match (self, scope) {
            (Self::Graph(g), 0) => &mut g.edit.structs,
            (Self::Graph(g), 1) => &mut g.edit.node_structs,
            (Self::Graph(g), _) => &mut g.edit.edge_structs,
            (Self::Model(m), _) => &mut m.edit.structs,
            (Self::Material(m), _) => &mut m.source_mut().structs,
            (Self::Atlas(a), _) => &mut a.0.structs,
            (Self::Gradient(g), 0) => &mut g.edit.structs,
            (Self::Gradient(g), _) => &mut g.edit.gradient_structs,
            (Self::Palette(p), 0) => &mut p.edit.structs,
            (Self::Palette(p), _) => &mut p.edit.color_structs,
        }
    }

    /// Every owner's values in `scope`.
    fn owners(&mut self, scope: usize) -> Vec<&mut Values> {
        match (self, scope) {
            (Self::Graph(g), 0) => vec![&mut g.edit.values],
            (Self::Graph(g), 1) => g.edit.nodes.iter_mut().map(|n| &mut n.values).collect(),
            (Self::Graph(g), _) => g.edit.edges.iter_mut().map(|e| &mut e.values).collect(),
            (Self::Model(m), _) => vec![&mut m.edit.values],
            (Self::Material(m), _) => vec![&mut m.source_mut().values],
            (Self::Atlas(a), _) => vec![&mut a.0.values],
            (Self::Gradient(g), 0) => vec![&mut g.edit.values],
            (Self::Gradient(g), _) => g.edit.gradients.iter_mut().map(|g| &mut g.values).collect(),
            (Self::Palette(p), 0) => vec![&mut p.edit.values],
            (Self::Palette(p), _) => p.edit.colors.iter_mut().map(|c| &mut c.values).collect(),
        }
    }

    /// The scopes as the build sees them, for problems.
    fn scopes(&mut self) -> Vec<structs::Scope<'_>> {
        let one = |label: &str, attachments, values| {
            vec![structs::Scope {
                scope: structs::ASSET,
                attachments,
                owners: vec![structs::Owner {
                    item: 0,
                    label: label.into(),
                    values,
                }],
            }]
        };
        match self {
            Self::Graph(g) => {
                let places = g.tree.then(|| vec![0; g.edit.nodes.len()]);
                let mut scopes = g.edit.scopes(places.as_deref());
                // Tree nodes are told apart by their labels here, not by place.
                if let Some(nodes) = scopes.get_mut(1) {
                    for (i, owner) in nodes.owners.iter_mut().enumerate() {
                        owner.item = i as u32;
                    }
                }
                scopes
            }
            Self::Model(m) => one("model", &m.edit.structs, &m.edit.values),
            Self::Material(m) => {
                let s = m.source_mut();
                one("material", &s.structs, &s.values)
            }
            Self::Atlas(a) => one("atlas", &a.0.structs, &a.0.values),
            Self::Gradient(g) => g.edit.scopes(),
            Self::Palette(p) => p.edit.scopes(),
        }
    }

    /// The owner whose values are edited: its scope, its place there and its label.
    fn owner(&self, view: &StructsView) -> (usize, usize, String) {
        match self {
            Self::Graph(g) => match g.chosen() {
                None if g.tree => (0, 0, "tree".into()),
                None => (0, 0, "graph".into()),
                Some(Chosen::Node(i)) => (1, i, format!("node {}", g.edit.nodes[i].id)),
                Some(Chosen::Edge(_)) if g.tree => (0, 0, "tree".into()),
                Some(Chosen::Edge(k)) => {
                    let e = &g.edit.edges[k];
                    (2, k, format!("edge {} -> {}", e.from, e.to))
                }
            },
            Self::Model(_) => (0, 0, "model".into()),
            Self::Material(_) => (0, 0, "material".into()),
            Self::Atlas(_) => (0, 0, "atlas".into()),
            Self::Gradient(_) | Self::Palette(_) => match view.owner {
                0 => (0, 0, "file".into()),
                n => (1, n - 1, self.item_label(n - 1)),
            },
        }
    }

    /// A gradient's or color's label: `<n>  <name>`, from 1.
    fn item_label(&self, i: usize) -> String {
        let name = match self {
            Self::Gradient(g) => g.edit.gradients.get(i).map(|g| g.name.clone()),
            Self::Palette(p) => p.edit.colors.get(i).map(|c| c.name.clone()),
            _ => None,
        }
        .unwrap_or_default();
        format!("{}  {name}", i + 1).trim_end().to_owned()
    }

    /// How many items a gradient file or palette has, for the owner dropdown.
    fn items(&self) -> Option<usize> {
        match self {
            Self::Gradient(g) => Some(g.edit.gradients.len()),
            Self::Palette(p) => Some(p.edit.colors.len()),
            _ => None,
        }
    }

    fn commit(&mut self, ui: &Ui) -> Option<StructsSave> {
        match self {
            Self::Graph(g) => graph::graph_commit(ui, g).map(StructsSave::File),
            Self::Model(m) => mesh_commit(ui, m).map(|a| StructsSave::Sidecar(Box::new(a))),
            Self::Material(m) => {
                material_edit::material_commit(ui, m).map(|s| StructsSave::File(s.to_toml()))
            }
            Self::Atlas(pair) => {
                let busy =
                    ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
                (!busy && pair.0 != pair.1).then(|| {
                    pair.1 = pair.0.clone();
                    StructsSave::Atlas(pair.0.to_toml())
                })
            }
            Self::Gradient(g) => gradient_commit(ui, g).map(|f| StructsSave::File(f.to_toml())),
            Self::Palette(p) => palette_commit(ui, p).map(|f| StructsSave::File(f.to_toml())),
        }
    }
}

/// The tab; returns what to write. `input` is the project's input directory.
pub(super) fn structs_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    view: &mut StructsView,
    input: Option<&Path>,
) -> Option<StructsSave> {
    // An atlas keeps its structs in the folder's atlas.toml, the others next to their file.
    let dir = match &inspected.atlas {
        Some(Ok(_)) => inspected.path.clone(),
        _ => inspected
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    };
    let cache = &mut inspected.structs_cache;
    let mut host = if let Some(Ok(g)) = &mut inspected.graph {
        Host::Graph(g)
    } else if let Some(Ok(m)) = &mut inspected.mesh {
        Host::Model(m)
    } else if let Some(Ok(m)) = &mut inspected.material {
        Host::Material(m)
    } else if let Some(Ok(a)) = &mut inspected.atlas {
        Host::Atlas(a)
    } else if let Some(Ok(g)) = &mut inspected.gradient {
        Host::Gradient(g)
    } else if let Some(Ok(p)) = &mut inspected.palette {
        Host::Palette(p)
    } else {
        placeholder(ui, "No structs");
        return None;
    };

    // Schemas are read again when the attachments name others.
    let mut key: Vec<String> = Vec::new();
    for s in 0..host.scope_names().len() {
        key.extend(host.attachments(s).iter().map(|a| a.schema.clone()));
    }
    key.sort();
    key.dedup();
    if cache.as_ref().is_none_or(|c| c.key != key) {
        let available = cache.take().map(|c| c.available).unwrap_or_else(|| {
            input
                .map(|input| {
                    adata::schemas_in(input)
                        .into_iter()
                        .map(|path| {
                            let structs = std::fs::read_to_string(&path)
                                .map_err(|e| e.to_string())
                                .and_then(|text| crate::aschema::Schema::parse(&text))
                                .map(|schema| {
                                    schema
                                        .types
                                        .iter()
                                        .filter(|t| {
                                            matches!(t.body, crate::aschema::Body::Struct { .. })
                                        })
                                        .map(|t| t.name.clone())
                                        .collect()
                                });
                            (adata::relative(&dir, &path), structs)
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        let schemas = structs::load(&dir, input, &host.scopes());
        *cache = Some(StructsCache {
            key,
            schemas,
            available,
            assets: input.map(adata::asset_files).unwrap_or_default(),
        });
    }
    let cache = cache.as_ref()?;
    let problems = structs::check(&host.scopes(), &cache.schemas, Some(&cache.assets));

    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                attached(ui, &mut host, cache, &problems);
                ui.add_space(12.0);
                values(ui, &mut host, view, cache, &problems);
            });
        });
    host.commit(ui)
}

/// One section per scope: its attachments, Detach and Attach struct.
fn attached(ui: &mut Ui, host: &mut Host, cache: &StructsCache, problems: &[Problem]) {
    let names = host.scope_names();
    let mut detach = None;
    for (scope, (title, _)) in names.iter().enumerate() {
        ui.add_space(if scope == 0 { 0.0 } else { 10.0 });
        ui.label(RichText::new(*title).size(14.0).color(TEXT_WEAK));
        let attachments = host.attachments(scope);
        if attachments.is_empty() {
            ui.weak("None");
        }
        for (k, a) in attachments.iter().enumerate() {
            let own: Vec<DataProblem> = problems
                .iter()
                .filter(|p| p.scope == scope as u32 && p.attachment == Some(k))
                .map(as_data_problem)
                .collect();
            let own: Vec<&DataProblem> = own.iter().collect();
            ui.horizontal(|ui| {
                ui.set_min_height(26.0);
                ui.label(RichText::new(&a.ty).monospace().color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Icon::X, "Detach", true).clicked() {
                        detach = Some((scope, k));
                    }
                    issues(ui, ("attached", scope, k), &own);
                    ui.weak(&a.schema);
                });
            });
        }
        let taken: Vec<String> = attachments.iter().map(|a| a.ty.clone()).collect();
        let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Attach struct");
        let mut chosen = None;
        ui.push_id(("attach", scope), |ui| {
            egui::containers::menu::MenuButton::from_button(add).ui(ui, |ui| {
                if cache.available.is_empty() {
                    ui.weak("No schemas in the input");
                }
                for (path, list) in &cache.available {
                    ui.menu_button(path, |ui| match list {
                        Err(e) => {
                            ui.add_enabled(false, egui::Button::new(e));
                        }
                        Ok(list) if list.is_empty() => {
                            ui.weak("No structs");
                        }
                        Ok(list) => {
                            for ty in list {
                                let free = !taken.contains(ty);
                                if ui.add_enabled(free, egui::Button::new(ty)).clicked() {
                                    chosen = Some(Attachment::new(path.clone(), ty.clone()));
                                }
                            }
                        }
                    });
                }
            });
        });
        if let Some(a) = chosen {
            host.attachments(scope).push(a);
        }
    }
    // Detaching drops the struct's values from every owner of the scope.
    if let Some((scope, k)) = detach {
        let ty = host.attachments(scope).remove(k).ty;
        for values in host.owners(scope) {
            values.shift_remove(&ty);
        }
    }
}

/// The values of the chosen owner, a card per struct of its scope.
fn values(
    ui: &mut Ui,
    host: &mut Host,
    view: &mut StructsView,
    cache: &StructsCache,
    problems: &[Problem],
) {
    if let Some(n) = host.items() {
        view.owner = view.owner.min(n);
    }
    let (scope, item, label) = host.owner(view);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Values of {label}"))
                .size(14.0)
                .color(TEXT),
        );
        if let Some(n) = host.items() {
            let labels: Vec<String> = std::iter::once("File".to_owned())
                .chain((0..n).map(|i| host.item_label(i)))
                .collect();
            egui::ComboBox::from_id_salt("struct_owner")
                .selected_text(&labels[view.owner])
                .show_ui(ui, |ui| {
                    for (i, text) in labels.iter().enumerate() {
                        ui.selectable_value(&mut view.owner, i, text);
                    }
                });
        }
    });
    // Problems of other owners, so nothing wrong is out of sight.
    for p in problems
        .iter()
        .filter(|p| p.owner.is_some() && p.owner.as_deref() != Some(label.as_str()))
    {
        let color = if p.warning { ACCENT } else { CHANNEL_R };
        ui.label(RichText::new(p.text()).size(12.0).color(color));
    }
    ui.add_space(6.0);
    let attachments = host.attachments(scope).clone();
    if attachments.is_empty() {
        let noun = host.scope_names()[scope].1;
        ui.weak(format!("No structs attached to {noun}s"));
        return;
    }
    let Some(values) = host.owners(scope).into_iter().nth(item) else {
        return;
    };
    for (k, a) in attachments.iter().enumerate() {
        Frame::new()
            .fill(BG_HEADER)
            .corner_radius(6)
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let resolved = structs::resolve_attachment(a, &cache.schemas);
                ui.horizontal(|ui| {
                    let title = ui.label(RichText::new(&a.ty).size(14.0).color(TEXT));
                    if let Ok((schema, si)) = &resolved
                        && !schema.types[*si].doc.is_empty()
                    {
                        title.on_hover_text(&schema.types[*si].doc);
                    }
                    ui.weak(&a.schema);
                });
                let (schema, si) = match resolved {
                    Ok(found) => found,
                    Err(e) => {
                        ui.label(RichText::new(e).color(CHANNEL_R));
                        return;
                    }
                };
                let mine: Vec<DataProblem> = problems
                    .iter()
                    .filter(|p| p.owner.as_deref() == Some(label.as_str()) && p.ty == a.ty)
                    .map(as_data_problem)
                    .collect();
                let mine: Vec<&DataProblem> = mine.iter().collect();
                let mut set: Map<String, Value> = values
                    .get(&a.ty)
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let before = set.clone();
                let refs = &cache.schemas[&a.schema].targets;
                // Each owner's struct values keep their own fold and editor state.
                let key = (scope * 1_000_000 + item) * 64 + k;
                ui.push_id(("struct_values", scope, item, k), |ui| {
                    struct_fields(
                        ui,
                        &mut view.fields,
                        key,
                        schema,
                        si,
                        &mut set,
                        &mine,
                        (&cache.assets, refs),
                    );
                });
                if set != before {
                    let set = adata::normalise(schema, si, &set, None);
                    if set.is_empty() {
                        values.shift_remove(&a.ty);
                    } else {
                        values.insert(a.ty.clone(), Value::Object(set));
                    }
                }
            });
        ui.add_space(8.0);
    }
}

/// A struct problem as the data editor shows it.
fn as_data_problem(p: &Problem) -> DataProblem {
    DataProblem {
        instance: 0,
        field: p.field.clone(),
        message: p.message.clone(),
        warning: p.warning,
    }
}
