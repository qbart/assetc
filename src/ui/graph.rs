//! The `.agraph` editor: the canvas grid of nodes and edges and the Inspector's Graph tab;
//! see docs/features/graphs.md.

use crate::agraph::{Edge, Node};
use crate::theme::CHANNEL_R;

use super::schema::commit_field;
use super::*;

/// Half a node's size, in grid units.
const HALF: (f64, f64) = (1.0, 0.375);
/// Pixels per unit to start with, and the zoom's limits.
const SCALE: f32 = 64.0;
const SCALE_RANGE: std::ops::RangeInclusive<f32> = 8.0..=512.0;
/// Snapping step with Shift, in units.
const SNAP: f64 = 0.25;
/// How near the pointer must be to a port or an edge, in points.
const REACH: f32 = 7.0;

/// What is chosen in a graph.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Chosen {
    Node(usize),
    Edge(usize),
}

/// A graph being edited.
#[derive(Default)]
pub(super) struct GraphEdit {
    pub(super) edit: Graph,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    pub(super) saved: Graph,
    chosen: Option<Chosen>,
    drag: Option<GraphDrag>,
}

impl GraphEdit {
    /// `graph` as read, keeping `previous`'s choice.
    pub(super) fn new(graph: Graph, previous: GraphEdit) -> Self {
        Self {
            edit: graph.clone(),
            saved: graph,
            ..previous
        }
    }
}

#[derive(Clone, Copy)]
enum GraphDrag {
    /// A node, held this far (in units) from its center.
    Move(usize, (f64, f64)),
    /// A rubber edge from a node's port.
    Connect(usize),
}

/// The canvas' view of a graph: the unit point at the canvas' center, and pixels per unit.
#[derive(Clone, Copy, Debug)]
pub(super) struct GraphView {
    center: (f64, f64),
    scale: f32,
}

impl Default for GraphView {
    fn default() -> Self {
        Self {
            center: (0.0, 0.0),
            scale: SCALE,
        }
    }
}

fn graph_commit(ui: &Ui, edit: &mut GraphEdit) -> Option<Graph> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

/// A position snapped to 0.25 (with Shift) or rounded to 0.001.
fn place(v: f64, snap: bool) -> f64 {
    if snap {
        (v / SNAP).round() * SNAP
    } else {
        (v * 1000.0).round() / 1000.0
    }
}

/// Distance from `p` to the segment `a`–`b`.
fn segment_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

/// The graph on its grid; returns the graph to write. See docs/features/graphs.md#canvas.
pub(super) fn canvas_graph(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    views: &mut HashMap<PathBuf, GraphView>,
) -> Option<Graph> {
    let view = views.entry(inspected.path.clone()).or_default();
    let edit = match &mut inspected.graph {
        Some(Ok(edit)) => edit,
        Some(Err(e)) => {
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                e.as_str(),
                FontId::proportional(14.0),
                CHANNEL_R,
            );
            return None;
        }
        None => return None,
    };
    let response = ui.interact(rect, ui.id().with("graph_canvas"), Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()));
    let shift = ui.input(|i| i.modifiers.shift);

    // View: zoom around the pointer, pan with the right or middle button, fit.
    if response.hovered()
        && let Some(p) = pointer
    {
        let (notches, pinch) = ui.input(|i| (wheel_notches(i), i.zoom_delta()));
        let factor = pinch * WHEEL_ZOOM.powf(notches);
        if factor != 1.0 {
            let before = view.scale;
            view.scale = (view.scale * factor).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
            let offset = p - rect.center();
            let dx = f64::from(offset.x) * (1.0 / f64::from(before) - 1.0 / f64::from(view.scale));
            let dy = f64::from(offset.y) * (1.0 / f64::from(before) - 1.0 / f64::from(view.scale));
            view.center = (view.center.0 + dx, view.center.1 + dy);
        }
    }
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        let d = response.drag_delta();
        let s = f64::from(view.scale);
        view.center = (
            view.center.0 - f64::from(d.x) / s,
            view.center.1 - f64::from(d.y) / s,
        );
    }
    if response.double_clicked_by(PointerButton::Secondary) && !edit.edit.nodes.is_empty() {
        let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for n in &edit.edit.nodes {
            lo = (lo.0.min(n.x - HALF.0 - 1.0), lo.1.min(n.y - HALF.1 - 1.0));
            hi = (hi.0.max(n.x + HALF.0 + 1.0), hi.1.max(n.y + HALF.1 + 1.0));
        }
        let fit =
            (f64::from(rect.width()) / (hi.0 - lo.0)).min(f64::from(rect.height()) / (hi.1 - lo.1));
        view.scale = (fit as f32).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        view.center = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
    }
    let view = *view;
    let s = f64::from(view.scale);
    let to_screen = |x: f64, y: f64| {
        rect.center()
            + vec2(
                ((x - view.center.0) * s) as f32,
                ((y - view.center.1) * s) as f32,
            )
    };
    let to_units = |p: egui::Pos2| {
        let d = p - rect.center();
        (
            view.center.0 + f64::from(d.x) / s,
            view.center.1 + f64::from(d.y) / s,
        )
    };
    let node_rect = |n: &Node| {
        Rect::from_min_max(
            to_screen(n.x - HALF.0, n.y - HALF.1),
            to_screen(n.x + HALF.0, n.y + HALF.1),
        )
    };
    let port = |n: &Node| to_screen(n.x + HALF.0, n.y);
    // The topmost (last drawn) node under a point.
    let node_at =
        |nodes: &[Node], p: egui::Pos2| nodes.iter().rposition(|n| node_rect(n).contains(p));
    // An edge's ends on screen: its From node's center, and its To node's border.
    let edge_ends = |nodes: &[Node], e: &Edge| {
        let a = nodes.iter().find(|n| n.id == e.from)?;
        let b = nodes.iter().find(|n| n.id == e.to)?;
        let (start, center) = (to_screen(a.x, a.y), to_screen(b.x, b.y));
        let half = node_rect(b).size() / 2.0;
        let dir = start - center;
        let t = (half.x / dir.x.abs().max(1e-6)).min(half.y / dir.y.abs().max(1e-6));
        Some((start, center + dir * t.min(1.0)))
    };

    // Choose, move, connect and add.
    let graph = &mut edit.edit;
    if response.double_clicked()
        && let Some(p) = pointer
        && node_at(&graph.nodes, p).is_none()
    {
        let (x, y) = to_units(p);
        let id = graph.free_id("node");
        graph
            .nodes
            .push(Node::new(id, place(x, shift), place(y, shift)));
        edit.chosen = Some(Chosen::Node(graph.nodes.len() - 1));
        edit.drag = None;
    } else if ui.input(|i| i.pointer.primary_pressed())
        && response.hovered()
        && let Some(p) = pointer
    {
        let on_port = graph
            .nodes
            .iter()
            .rposition(|n| (port(n) - p).length() <= REACH);
        if let Some(i) = on_port {
            edit.drag = Some(GraphDrag::Connect(i));
            edit.chosen = Some(Chosen::Node(i));
        } else if let Some(i) = node_at(&graph.nodes, p) {
            let (x, y) = to_units(p);
            let n = &graph.nodes[i];
            edit.drag = Some(GraphDrag::Move(i, (x - n.x, y - n.y)));
            edit.chosen = Some(Chosen::Node(i));
        } else {
            edit.chosen = graph
                .edges
                .iter()
                .enumerate()
                .filter_map(|(k, e)| Some((k, edge_ends(&graph.nodes, e)?)))
                .map(|(k, (a, b))| (k, segment_distance(p, a, b)))
                .filter(|(_, d)| *d <= REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(k, _)| Chosen::Edge(k));
            edit.drag = None;
        }
    }
    let down = ui.input(|i| i.pointer.primary_down());
    match (edit.drag, pointer) {
        (Some(GraphDrag::Move(i, (ox, oy))), Some(p)) if down && i < graph.nodes.len() => {
            let (x, y) = to_units(p);
            graph.nodes[i].x = place(x - ox, shift);
            graph.nodes[i].y = place(y - oy, shift);
        }
        (Some(GraphDrag::Connect(i)), Some(p)) if !down && i < graph.nodes.len() => {
            // Released: an edge to the node under the pointer, when it may be added.
            if let Some(j) = node_at(&graph.nodes, p) {
                let (from, to) = (graph.nodes[i].id.clone(), graph.nodes[j].id.clone());
                if graph.can_connect(&from, &to) {
                    graph.edges.push(Edge {
                        from,
                        to,
                        ..Edge::default()
                    });
                    edit.chosen = Some(Chosen::Edge(graph.edges.len() - 1));
                }
            }
            edit.drag = None;
        }
        (Some(_), _) if !down => edit.drag = None,
        _ => {}
    }
    if edit.drag.is_some() {
        ui.ctx().request_repaint();
    }

    // Grid: quarter lines while 8 px apart, unit lines, and the axes.
    let painter = ui.painter_at(rect);
    let (lo, hi) = (to_units(rect.min), to_units(rect.max));
    let lines = |step: f64, stroke: Stroke| {
        let mut k = (lo.0 / step).floor();
        while k * step <= hi.0 {
            painter.vline(to_screen(k * step, 0.0).x, rect.y_range(), stroke);
            k += 1.0;
        }
        let mut k = (lo.1 / step).floor();
        while k * step <= hi.1 {
            painter.hline(rect.x_range(), to_screen(0.0, k * step).y, stroke);
            k += 1.0;
        }
    };
    if view.scale * SNAP as f32 >= 8.0 {
        lines(SNAP, Stroke::new(1.0, STROKE));
    }
    lines(1.0, Stroke::new(1.0, TEXT_WEAK.gamma_multiply(0.3)));
    let origin = to_screen(0.0, 0.0);
    let axis = Stroke::new(1.0, TEXT_WEAK.gamma_multiply(0.6));
    painter.vline(origin.x, rect.y_range(), axis);
    painter.hline(rect.x_range(), origin.y, axis);

    // Edges, then the rubber edge, then nodes over them.
    for (k, e) in graph.edges.iter().enumerate() {
        let Some((a, b)) = edge_ends(&graph.nodes, e) else {
            continue;
        };
        let chosen = edit.chosen == Some(Chosen::Edge(k));
        let (width, color) = if chosen {
            (2.5, ACCENT)
        } else {
            (1.5, TEXT_WEAK)
        };
        painter.line_segment([a, b], Stroke::new(width, color));
        let dir = (b - a).normalized();
        let side = vec2(-dir.y, dir.x) * 5.0;
        painter.add(egui::Shape::convex_polygon(
            vec![b, b - dir * 10.0 + side, b - dir * 10.0 - side],
            color,
            Stroke::NONE,
        ));
    }
    if let (Some(GraphDrag::Connect(i)), Some(p)) = (edit.drag, pointer)
        && let Some(n) = graph.nodes.get(i)
    {
        painter.line_segment([port(n), p], Stroke::new(1.5, ACCENT));
    }
    let zoom = view.scale / SCALE;
    for (i, n) in graph.nodes.iter().enumerate() {
        let r = node_rect(n);
        let chosen = edit.chosen == Some(Chosen::Node(i));
        painter.rect_filled(r, 6.0 * zoom, BG_HEADER);
        let stroke = if chosen {
            Stroke::new(2.0, ACCENT)
        } else {
            Stroke::new(1.0, STROKE)
        };
        painter.rect_stroke(r, 6.0 * zoom, stroke, StrokeKind::Inside);
        let size = (14.0 * zoom).max(8.0);
        painter.text(
            r.center(),
            Align2::CENTER_CENTER,
            n.title(),
            FontId::proportional(size),
            TEXT,
        );
        let hovered = pointer.is_some_and(|p| (port(n) - p).length() <= REACH);
        painter.circle_filled(port(n), 5.0, if hovered { ACCENT } else { TEXT_WEAK });
    }
    painter.text(
        rect.left_bottom() + vec2(8.0, -8.0),
        Align2::LEFT_BOTTOM,
        "Double-click to add a node · drag a port onto a node to connect · Shift snaps to 0.25",
        FontId::proportional(12.0),
        TEXT_WEAK,
    );
    graph_commit(ui, edit)
}

/// The chosen node or edge, or the node list; returns the graph to write. See
/// docs/features/graphs.md#graph-tab.
pub(super) fn graph_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    view: &mut GraphView,
) -> Option<Graph> {
    let edit = match &mut inspected.graph {
        Some(Ok(edit)) => edit,
        Some(Err(e)) => {
            placeholder(ui, e);
            return None;
        }
        None => {
            placeholder(ui, "No graph");
            return None;
        }
    };
    edit.chosen = edit.chosen.filter(|c| match *c {
        Chosen::Node(i) => i < edit.edit.nodes.len(),
        Chosen::Edge(k) => k < edit.edit.edges.len(),
    });
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                for problem in edit.edit.check() {
                    ui.label(RichText::new(problem).size(12.0).color(CHANNEL_R));
                }
                match edit.chosen {
                    None => node_list(ui, edit, view),
                    Some(Chosen::Node(i)) => node_fields(ui, edit, i),
                    Some(Chosen::Edge(k)) => edge_fields(ui, edit, k),
                }
            });
        });
    graph_commit(ui, edit)
}

/// Every node, to choose one, and Add node.
fn node_list(ui: &mut Ui, edit: &mut GraphEdit, view: &mut GraphView) {
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add node");
    if ui.add(add).clicked() {
        let id = edit.edit.free_id("node");
        let (x, y) = view.center;
        edit.edit
            .nodes
            .push(Node::new(id, place(x, true), place(y, true)));
        edit.chosen = Some(Chosen::Node(edit.edit.nodes.len() - 1));
        return;
    }
    ui.add_space(8.0);
    for (i, n) in edit.edit.nodes.iter().enumerate() {
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 4, BG_HEADER);
        }
        ui.painter().text(
            rect.left_center() + vec2(6.0, 0.0),
            Align2::LEFT_CENTER,
            &n.id,
            FontId::monospace(13.0),
            TEXT,
        );
        if !n.label.is_empty() {
            ui.painter().text(
                rect.right_center() - vec2(6.0, 0.0),
                Align2::RIGHT_CENTER,
                &n.label,
                FontId::proportional(13.0),
                TEXT_WEAK,
            );
        }
        if response.clicked() {
            edit.chosen = Some(Chosen::Node(i));
            view.center = (n.x, n.y);
        }
    }
}

fn node_fields(ui: &mut Ui, edit: &mut GraphEdit, i: usize) {
    let graph = &mut edit.edit;
    let old = graph.nodes[i].id.clone();
    prop_row(ui, "Id", |ui, width| {
        if let Some(new) = commit_field(ui, ("node_id", i), &old, width, "id")
            && aschema::is_identifier(&new)
            && graph.node_index(&new).is_none()
        {
            graph.rename_node(&old, &new);
        }
    });
    let node = &mut graph.nodes[i];
    prop_row(ui, "Label", |ui, width| {
        ui.add_sized(
            vec2(width, 22.0),
            egui::TextEdit::singleline(&mut node.label),
        );
    });
    for (label, value) in [("X", &mut node.x), ("Y", &mut node.y)] {
        prop_row(ui, label, |ui, width| {
            let field = egui::DragValue::new(value).speed(0.01).fixed_decimals(3);
            ui.add_sized(vec2(width, 22.0), field);
        });
    }
    ui.add_space(6.0);
    ui.label(RichText::new("Data").size(13.0).color(TEXT_WEAK));
    properties_editor(ui, &mut node.data);
    ui.add_space(6.0);
    ui.label(RichText::new("Edges").size(13.0).color(TEXT_WEAK));
    let id = graph.nodes[i].id.clone();
    let mut chosen = None;
    for (k, e) in graph.edges.iter().enumerate() {
        let text = if e.from == id {
            format!("→ {}", e.to)
        } else if e.to == id {
            format!("← {}", e.from)
        } else {
            continue;
        };
        if ui
            .add(egui::Label::new(RichText::new(text).monospace()).sense(Sense::click()))
            .clicked()
        {
            chosen = Some(Chosen::Edge(k));
        }
    }
    ui.add_space(8.0);
    let delete = egui::Button::image_and_text(Icon::Trash2.image(14.0, TEXT), "Delete node");
    if ui.add(delete).clicked() {
        graph.remove_node(i);
        edit.chosen = None;
        return;
    }
    if chosen.is_some() {
        edit.chosen = chosen;
    }
}

fn edge_fields(ui: &mut Ui, edit: &mut GraphEdit, k: usize) {
    let graph = &mut edit.edit;
    let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let (from, to) = (graph.edges[k].from.clone(), graph.edges[k].to.clone());
    let mut new_from = from.clone();
    let mut new_to = to.clone();
    // Ends that would loop or repeat an edge are not offered.
    let free = |a: &str, b: &str| {
        a != b
            && !graph
                .edges
                .iter()
                .enumerate()
                .any(|(j, e)| j != k && e.from == a && e.to == b)
    };
    prop_row(ui, "From", |ui, width| {
        egui::ComboBox::from_id_salt("edge_from")
            .width(width - 8.0)
            .selected_text(&new_from)
            .show_ui(ui, |ui| {
                for id in ids.iter().filter(|id| free(id, &to)) {
                    ui.selectable_value(&mut new_from, id.clone(), id);
                }
            });
    });
    prop_row(ui, "To", |ui, width| {
        egui::ComboBox::from_id_salt("edge_to")
            .width(width - 8.0)
            .selected_text(&new_to)
            .show_ui(ui, |ui| {
                for id in ids.iter().filter(|id| free(&from, id)) {
                    ui.selectable_value(&mut new_to, id.clone(), id);
                }
            });
    });
    let edge = &mut graph.edges[k];
    (edge.from, edge.to) = (new_from, new_to);
    ui.add_space(6.0);
    ui.label(RichText::new("Data").size(13.0).color(TEXT_WEAK));
    properties_editor(ui, &mut edge.data);
    ui.add_space(8.0);
    let delete = egui::Button::image_and_text(Icon::Trash2.image(14.0, TEXT), "Delete edge");
    if ui.add(delete).clicked() {
        graph.edges.remove(k);
        edit.chosen = None;
    }
}
