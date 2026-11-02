//! The `.agraph` editor: the canvas grid of nodes and edges and the Inspector's Graph tab;
//! see docs/features/graphs.md.

use crate::agraph::{Edge, Node};
use crate::theme::CHANNEL_R;

use super::schema::commit_field;
use super::*;

/// A node's radius, in grid units.
const RADIUS: f64 = 0.5;
/// Pixels per unit to start with, and the zoom's limits.
const SCALE: f32 = 64.0;
const SCALE_RANGE: std::ops::RangeInclusive<f32> = 8.0..=512.0;
/// Snapping step with Shift, in units.
const SNAP: f64 = 0.25;
/// How near the pointer must be to an edge, in points.
const REACH: f32 = 7.0;
/// The rim edges are dragged from, in points.
const RIM: f32 = 8.0;
/// How far an edge's middle bends off the straight line, per edge with the same ends, in units.
const BEND: f32 = 0.25;
const ARROW: f32 = 10.0;

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
    /// A node's label being edited on the canvas, and whether its field still needs focus.
    label: Option<(usize, String, bool)>,
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
    /// A rubber edge from a node's rim, and whether the pointer has left that node.
    Connect(usize, bool),
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

/// A point of the cubic Bézier `c` at `t`.
fn bezier(c: &[egui::Pos2; 4], t: f32) -> egui::Pos2 {
    let u = 1.0 - t;
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    pos2(
        c.iter().zip(w).map(|(p, w)| p.x * w).sum(),
        c.iter().zip(w).map(|(p, w)| p.y * w).sum(),
    )
}

/// An edge as drawn, in points: its curve from border to border, where its arrow points, and
/// the middle its label sits on.
struct EdgeShape {
    line: Vec<egui::Pos2>,
    tip: egui::Pos2,
    dir: Vec2,
    middle: egui::Pos2,
}

/// The shape of an edge between circles at `a` and `b` (radius `r` points), the `slot`th
/// with the same ends; `bend` is [`BEND`] in points. See docs/features/graphs.md#canvas.
fn edge_shape(a: egui::Pos2, b: egui::Pos2, r: f32, slot: usize, bend: f32) -> EdgeShape {
    let n = slot as f32;
    let (curve, t0, t1) = if (b - a).length() < 1e-3 {
        // A loop over the node, larger for each further one.
        let at = |deg: f32, d: f32| a + vec2(deg.to_radians().cos(), deg.to_radians().sin()) * d;
        let h = r * (1.8 + 0.9 * n);
        let curve = [
            at(-115.0, r),
            at(-125.0, r + h),
            at(-55.0, r + h),
            at(-65.0, r),
        ];
        (curve, 0.0, 1.0)
    } else {
        // A quadratic bending to the right of its direction, raised to a cubic.
        let d = (b - a).normalized();
        let control = a + (b - a) / 2.0 + vec2(-d.y, d.x) * 2.0 * bend * (n + 1.0);
        let curve = [
            a,
            a + (control - a) * (2.0 / 3.0),
            b + (control - b) * (2.0 / 3.0),
            b,
        ];
        // Where it leaves `a`'s circle and enters `b`'s.
        let crossing = |from: f32, to: f32, center: egui::Pos2| {
            let (mut inside, mut outside) = (from, to);
            for _ in 0..20 {
                let mid = (inside + outside) / 2.0;
                if (bezier(&curve, mid) - center).length() < r {
                    inside = mid;
                } else {
                    outside = mid;
                }
            }
            outside
        };
        (curve, crossing(0.0, 0.5, a), crossing(1.0, 0.5, b))
    };
    let tip = bezier(&curve, t1);
    let dir = (tip - bezier(&curve, t1 - 0.02 * (t1 - t0).max(0.05))).normalized();
    let mut line: Vec<egui::Pos2> = (0..=24)
        .map(|i| bezier(&curve, t0 + (t1 - t0) * i as f32 / 24.0))
        .collect();
    // Stop under the arrowhead, not through its point.
    if let Some(last) = line.last_mut() {
        *last = tip - dir * ARROW * 0.8;
    }
    EdgeShape {
        line,
        tip,
        dir,
        middle: bezier(&curve, (t0 + t1) / 2.0),
    }
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
            lo = (lo.0.min(n.x - RADIUS - 1.0), lo.1.min(n.y - RADIUS - 1.0));
            hi = (hi.0.max(n.x + RADIUS + 1.0), hi.1.max(n.y + RADIUS + 1.0));
        }
        let fit =
            (f64::from(rect.width()) / (hi.0 - lo.0)).min(f64::from(rect.height()) / (hi.1 - lo.1));
        view.scale = (fit as f32).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        view.center = ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
    }
    let view = *view;
    let s = f64::from(view.scale);
    let r = (RADIUS * s) as f32;
    let rim = if r < 2.0 * RIM { r / 2.0 } else { RIM };
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
    let center = |n: &Node| to_screen(n.x, n.y);
    // The topmost (last drawn) node under a point, and how far from its center.
    let node_at = |nodes: &[Node], p: egui::Pos2| {
        nodes
            .iter()
            .rposition(|n| (center(n) - p).length() <= r)
            .map(|i| (i, (center(&nodes[i]) - p).length()))
    };
    let shapes = |graph: &Graph| -> Vec<Option<EdgeShape>> {
        graph
            .edges
            .iter()
            .enumerate()
            .map(|(k, e)| {
                let a = graph.nodes.iter().find(|n| n.id == e.from)?;
                let b = graph.nodes.iter().find(|n| n.id == e.to)?;
                let slot = graph.edges[..k]
                    .iter()
                    .filter(|o| o.from == e.from && o.to == e.to)
                    .count();
                Some(edge_shape(center(a), center(b), r, slot, BEND * view.scale))
            })
            .collect()
    };

    // Choose, move, connect, add and rename.
    let graph = &mut edit.edit;
    if response.double_clicked()
        && let Some(p) = pointer
    {
        if let Some((i, _)) = node_at(&graph.nodes, p) {
            edit.label = Some((i, graph.nodes[i].label.clone(), true));
            edit.chosen = Some(Chosen::Node(i));
        } else {
            let (x, y) = to_units(p);
            let id = graph.free_id("node");
            graph
                .nodes
                .push(Node::new(id, place(x, shift), place(y, shift)));
            edit.chosen = Some(Chosen::Node(graph.nodes.len() - 1));
        }
        edit.drag = None;
    } else if ui.input(|i| i.pointer.primary_pressed())
        && response.hovered()
        && let Some(p) = pointer
    {
        if let Some((i, d)) = node_at(&graph.nodes, p) {
            edit.drag = Some(if d >= r - rim {
                GraphDrag::Connect(i, false)
            } else {
                let (x, y) = to_units(p);
                let n = &graph.nodes[i];
                GraphDrag::Move(i, (x - n.x, y - n.y))
            });
            edit.chosen = Some(Chosen::Node(i));
        } else {
            edit.chosen = shapes(graph)
                .iter()
                .enumerate()
                .filter_map(|(k, shape)| {
                    let line = &shape.as_ref()?.line;
                    let d = line
                        .windows(2)
                        .map(|w| segment_distance(p, w[0], w[1]))
                        .fold(f32::MAX, f32::min);
                    Some((k, d))
                })
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
        (Some(GraphDrag::Connect(i, left)), Some(p)) if i < graph.nodes.len() => {
            let target = node_at(&graph.nodes, p).map(|(j, _)| j);
            let left = left || target != Some(i);
            if down {
                edit.drag = Some(GraphDrag::Connect(i, left));
            } else {
                // Released on a node: a new edge to it (to itself once the pointer left it).
                if let Some(j) = target
                    && (j != i || left)
                {
                    let (from, to) = (graph.nodes[i].id.clone(), graph.nodes[j].id.clone());
                    graph.edges.push(Edge::new(from, to));
                    edit.chosen = Some(Chosen::Edge(graph.edges.len() - 1));
                }
                edit.drag = None;
            }
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

    // Edges, then the rubber edge, then nodes over them, then edge labels.
    let zoom = view.scale / SCALE;
    let shapes = shapes(graph);
    for (k, shape) in shapes.iter().enumerate() {
        let Some(shape) = shape else {
            continue;
        };
        let chosen = edit.chosen == Some(Chosen::Edge(k));
        let (width, color) = if chosen {
            (2.5, ACCENT)
        } else {
            (1.5, TEXT_WEAK)
        };
        painter.add(egui::Shape::line(
            shape.line.clone(),
            Stroke::new(width, color),
        ));
        let side = vec2(-shape.dir.y, shape.dir.x) * ARROW / 2.0;
        let base = shape.tip - shape.dir * ARROW;
        painter.add(egui::Shape::convex_polygon(
            vec![shape.tip, base + side, base - side],
            color,
            Stroke::NONE,
        ));
    }
    if let (Some(GraphDrag::Connect(i, _)), Some(p)) = (edit.drag, pointer)
        && let Some(n) = graph.nodes.get(i)
    {
        let c = center(n);
        let start = c + (p - c).normalized() * r;
        painter.line_segment([start, p], Stroke::new(1.5, ACCENT));
    }
    let hovered_rim = match (edit.drag, pointer) {
        (None, Some(p)) if response.hovered() => node_at(&graph.nodes, p)
            .filter(|&(_, d)| d >= r - rim)
            .map(|(i, _)| i),
        (Some(GraphDrag::Connect(i, _)), _) => Some(i),
        _ => None,
    };
    if hovered_rim.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        let c = center(n);
        painter.circle_filled(c, r, BG_HEADER);
        let stroke = if edit.chosen == Some(Chosen::Node(i)) {
            Stroke::new(2.0, ACCENT)
        } else {
            Stroke::new(1.0, STROKE)
        };
        painter.circle_stroke(c, r - stroke.width / 2.0, stroke);
        if hovered_rim == Some(i) {
            painter.circle_stroke(c, r, Stroke::new(2.0, ACCENT));
        }
        if edit.label.as_ref().is_none_or(|(j, _, _)| *j != i) {
            painter.text(
                c,
                Align2::CENTER_CENTER,
                n.title(),
                FontId::proportional((13.0 * zoom).max(8.0)),
                TEXT,
            );
        }
    }
    let size = 12.0 * zoom;
    if size >= 6.0 {
        for (k, (e, shape)) in graph.edges.iter().zip(&shapes).enumerate() {
            let Some(shape) = shape else {
                continue;
            };
            let text = match (e.label.is_empty(), e.weight) {
                (true, 1) => continue,
                (true, w) => w.to_string(),
                (false, 1) => e.label.clone(),
                (false, w) => format!("{} · {w}", e.label),
            };
            let color = if edit.chosen == Some(Chosen::Edge(k)) {
                ACCENT
            } else {
                TEXT_WEAK
            };
            let galley = painter.layout_no_wrap(text, FontId::proportional(size.max(8.0)), color);
            let at = Rect::from_center_size(shape.middle, galley.size());
            painter.rect_filled(at.expand(2.0), 3, BG_CENTER);
            painter.galley(at.min, galley, color);
        }
    }
    painter.text(
        rect.left_bottom() + vec2(8.0, -8.0),
        Align2::LEFT_BOTTOM,
        "Double-click to add a node · double-click a node to rename · \
         drag a node's rim onto a node to connect · Shift snaps to 0.25",
        FontId::proportional(12.0),
        TEXT_WEAK,
    );

    // The label being edited, over its node.
    if let Some((i, text, focus)) = &mut edit.label {
        if let Some(n) = graph.nodes.get_mut(*i) {
            let at = Rect::from_center_size(center(n), vec2((2.0 * r).max(120.0), 24.0));
            let field = egui::TextEdit::singleline(text)
                .id(ui.id().with("graph_label"))
                .horizontal_align(Align::Center);
            let response = ui.put(at, field);
            if *focus {
                response.request_focus();
                *focus = false;
            } else if !response.has_focus() {
                if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    n.label = text.clone();
                }
                edit.label = None;
            }
        } else {
            edit.label = None;
        }
    }
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
                let problems = edit.edit.check();
                let problems = |ui: &mut Ui| {
                    for problem in &problems {
                        ui.label(RichText::new(problem).size(12.0).color(CHANNEL_R));
                    }
                };
                match edit.chosen {
                    None => {
                        problems(ui);
                        node_list(ui, edit, view);
                    }
                    Some(Chosen::Node(i)) => {
                        node_fields(ui, edit, i);
                        problems(ui);
                    }
                    Some(Chosen::Edge(k)) => {
                        edge_fields(ui, edit, k);
                        problems(ui);
                    }
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

/// A pane's title, with Delete at its top right; returns whether Delete was clicked.
fn pane_header(ui: &mut Ui, title: &str, tooltip: &str) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(14.0).color(TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            icon_button(ui, Icon::Trash2, tooltip, true).clicked()
        })
        .inner
    })
    .inner
}

fn node_fields(ui: &mut Ui, edit: &mut GraphEdit, i: usize) {
    let graph = &mut edit.edit;
    if pane_header(ui, "Node", "Delete node") {
        graph.remove_node(i);
        edit.chosen = None;
        edit.label = None;
        return;
    }
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
        let mut text = if e.from == id && e.to == id {
            "↻".to_owned()
        } else if e.from == id {
            format!("→ {}", e.to)
        } else if e.to == id {
            format!("← {}", e.from)
        } else {
            continue;
        };
        if !e.label.is_empty() {
            text = format!("{text}  {}", e.label);
        }
        if ui
            .add(egui::Label::new(RichText::new(text).monospace()).sense(Sense::click()))
            .clicked()
        {
            chosen = Some(Chosen::Edge(k));
        }
    }
    if chosen.is_some() {
        edit.chosen = chosen;
    }
}

fn edge_fields(ui: &mut Ui, edit: &mut GraphEdit, k: usize) {
    let graph = &mut edit.edit;
    if pane_header(ui, "Edge", "Delete edge") {
        graph.edges.remove(k);
        edit.chosen = None;
        return;
    }
    let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let edge = &mut graph.edges[k];
    for (label, end) in [("From", &mut edge.from), ("To", &mut edge.to)] {
        prop_row(ui, label, |ui, width| {
            egui::ComboBox::from_id_salt(("edge_end", label))
                .width(width - 8.0)
                .selected_text(end.as_str())
                .show_ui(ui, |ui| {
                    for id in &ids {
                        ui.selectable_value(end, id.clone(), id);
                    }
                });
        });
    }
    prop_row(ui, "Label", |ui, width| {
        ui.add_sized(
            vec2(width, 22.0),
            egui::TextEdit::singleline(&mut edge.label),
        );
    });
    prop_row(ui, "Weight", |ui, width| {
        ui.add_sized(vec2(width, 22.0), egui::DragValue::new(&mut edge.weight));
    });
    ui.add_space(6.0);
    ui.label(RichText::new("Data").size(13.0).color(TEXT_WEAK));
    properties_editor(ui, &mut edge.data);
}
