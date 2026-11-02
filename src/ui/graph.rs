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
/// The body, pressed to move a node, as a share of the radius; the rim, edges are dragged
/// from, is the rest of the circle and this many points outside it.
const BODY: f32 = 0.6;
const RIM: f32 = 8.0;
/// Edges between the same two nodes: their spacing across the axis and their farthest
/// offset, in units.
const SPREAD: f32 = 0.2;
const SPREAD_MAX: f32 = 0.4;
const ARROW: f32 = 10.0;
/// How near the pointer must be to an edge's start or tip to grab it, in points.
const END: f32 = 8.0;

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
    /// An edge's end detached: its tip (true) or its start.
    Detach(usize, bool),
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

/// How an edge leaves its node: a loop (the how-manyth of that node), or toward another node
/// this many units right of its direction.
#[derive(Clone, Copy)]
enum Link {
    Loop(usize),
    Across(f32),
}

/// The `k`th edge's offset across the axis, in units, of `f` edges one way between two nodes
/// with `g` edges the other way. See docs/features/graphs.md#canvas.
fn spread(k: usize, f: usize, g: usize) -> f32 {
    let (k, f, g) = (k as f32, f as f32, g as f32);
    if g == 0.0 {
        let step = if f > 1.0 {
            SPREAD.min(2.0 * SPREAD_MAX / (f - 1.0))
        } else {
            SPREAD
        };
        (k - (f - 1.0) / 2.0) * step
    } else {
        (k + 0.5) * SPREAD.min(SPREAD_MAX / (f.max(g) - 0.5))
    }
}

/// The shape of an edge between circles at `a` and `b` (radius `r` points), `unit` points per
/// unit: an S-shaped link along the axis the nodes are farther apart on, or a loop. See
/// docs/features/graphs.md#canvas.
fn edge_shape(a: egui::Pos2, b: egui::Pos2, r: f32, link: Link, unit: f32) -> EdgeShape {
    let curve = match link {
        Link::Loop(n) => {
            // A loop over the node, larger for each further one.
            let at =
                |deg: f32, d: f32| a + vec2(deg.to_radians().cos(), deg.to_radians().sin()) * d;
            let h = r * (1.8 + 0.9 * n as f32);
            [
                at(-115.0, r),
                at(-125.0, r + h),
                at(-55.0, r + h),
                at(-65.0, r),
            ]
        }
        Link::Across(off) => {
            let d = b - a;
            let axis = if d.x.abs() >= d.y.abs() {
                vec2(d.x.signum(), 0.0)
            } else {
                vec2(0.0, d.y.signum())
            };
            let right = vec2(-axis.y, axis.x);
            let off = (off * unit).clamp(-0.8 * r, 0.8 * r);
            let along = (r * r - off * off).sqrt();
            let start = a + right * off + axis * along;
            let end = b + right * off - axis * along;
            let reach = ((end - start).dot(axis).abs() / 2.0).max(0.25 * unit);
            [start, start + axis * reach, end - axis * reach, end]
        }
    };
    let tip = curve[3];
    let dir = (tip - bezier(&curve, 0.98)).normalized();
    let mut line: Vec<egui::Pos2> = (0..=32).map(|i| bezier(&curve, i as f32 / 32.0)).collect();
    // Stop under the arrowhead, not through its point.
    if let Some(last) = line.last_mut() {
        *last = tip - dir * ARROW * 0.8;
    }
    EdgeShape {
        line,
        tip,
        dir,
        middle: bezier(&curve, 0.5),
    }
}

/// An arrowhead pointing along `dir` with its point at `tip`.
fn arrowhead(painter: &egui::Painter, tip: egui::Pos2, dir: Vec2, color: egui::Color32) {
    let side = vec2(-dir.y, dir.x) * ARROW / 2.0;
    let base = tip - dir * ARROW;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, base + side, base - side],
        color,
        Stroke::NONE,
    ));
}

/// The graph on its grid; returns the graph to write. See docs/features/graphs.md#canvas.
pub(super) fn canvas_graph(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    views: &mut HashMap<PathBuf, GraphView>,
    keys: bool,
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
    // The node under a point (its circle, else its rim outside it; the topmost, else the
    // nearest), and how far from its center.
    let node_at = |nodes: &[Node], p: egui::Pos2| {
        let distance = |i: usize| (center(&nodes[i]) - p).length();
        nodes
            .iter()
            .rposition(|n| (center(n) - p).length() <= r)
            .or_else(|| {
                (0..nodes.len())
                    .filter(|&i| distance(i) <= r + RIM)
                    .min_by(|&i, &j| distance(i).total_cmp(&distance(j)))
            })
            .map(|i| (i, distance(i)))
    };
    let unit = view.scale;
    // Edges each way between every two nodes.
    let counts = |graph: &Graph| {
        let mut counts: HashMap<(String, String), usize> = HashMap::new();
        for e in &graph.edges {
            *counts.entry((e.from.clone(), e.to.clone())).or_default() += 1;
        }
        counts
    };
    let shapes = |graph: &Graph| -> Vec<Option<EdgeShape>> {
        let counts = counts(graph);
        let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
        graph
            .edges
            .iter()
            .map(|e| {
                let k = seen.entry((&e.from, &e.to)).or_default();
                let slot = *k;
                *k += 1;
                let a = graph.nodes.iter().find(|n| n.id == e.from)?;
                let b = graph.nodes.iter().find(|n| n.id == e.to)?;
                let link = if e.from == e.to {
                    Link::Loop(slot)
                } else {
                    let count = |f: &str, t: &str| {
                        counts
                            .get(&(f.to_owned(), t.to_owned()))
                            .copied()
                            .unwrap_or(0)
                    };
                    let (f, g) = (count(&e.from, &e.to), count(&e.to, &e.from));
                    Link::Across(spread(slot, f, g))
                };
                Some(edge_shape(center(a), center(b), r, link, unit))
            })
            .collect()
    };
    // The edge from node `a` to node `b` as it would be added, the last of its ends; `skip`
    // is an edge being moved, not counted.
    let planned = |graph: &Graph, a: usize, b: usize, skip: Option<usize>| {
        let (from, to) = (&graph.nodes[a].id, &graph.nodes[b].id);
        let count = |f: &str, t: &str| {
            graph
                .edges
                .iter()
                .enumerate()
                .filter(|(k, e)| Some(*k) != skip && e.from == f && e.to == t)
                .count()
        };
        let link = if a == b {
            Link::Loop(count(from, from))
        } else {
            let f = count(from, to);
            Link::Across(spread(f, f + 1, count(to, from)))
        };
        edge_shape(
            center(&graph.nodes[a]),
            center(&graph.nodes[b]),
            r,
            link,
            unit,
        )
    };
    // The edge end nearest a point, within reach: its edge and whether it is the tip.
    let end_at = |shapes: &[Option<EdgeShape>], p: egui::Pos2| {
        shapes
            .iter()
            .enumerate()
            .filter_map(|(k, shape)| Some((k, shape.as_ref()?)))
            .flat_map(|(k, shape)| [(k, false, shape.line[0]), (k, true, shape.tip)])
            .map(|(k, tip, at)| (k, tip, (at - p).length()))
            .filter(|(_, _, d)| *d <= END)
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(k, tip, _)| (k, tip))
    };

    // Choose, move, connect, detach, add, rename and remove.
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
        if let Some((k, tip)) = end_at(&shapes(graph), p) {
            edit.drag = Some(GraphDrag::Detach(k, tip));
            edit.chosen = Some(Chosen::Edge(k));
        } else if let Some((i, d)) = node_at(&graph.nodes, p) {
            edit.drag = Some(if d >= BODY * r {
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
    // The node a rubber edge would end on: any node, its start once the pointer has left it.
    let mut target = None;
    match (edit.drag, pointer) {
        (Some(GraphDrag::Move(i, (ox, oy))), Some(p)) if down && i < graph.nodes.len() => {
            let (x, y) = to_units(p);
            graph.nodes[i].x = place(x - ox, shift);
            graph.nodes[i].y = place(y - oy, shift);
        }
        (Some(GraphDrag::Connect(i, left)), Some(p)) if i < graph.nodes.len() => {
            let under = node_at(&graph.nodes, p).map(|(j, _)| j);
            let left = left || under != Some(i);
            target = under.filter(|&j| j != i || left);
            if down {
                edit.drag = Some(GraphDrag::Connect(i, left));
            } else {
                // Released on a target: a new edge to it.
                if let Some(j) = target {
                    let (from, to) = (graph.nodes[i].id.clone(), graph.nodes[j].id.clone());
                    graph.edges.push(Edge::new(from, to));
                    edit.chosen = Some(Chosen::Edge(graph.edges.len() - 1));
                }
                edit.drag = None;
                target = None;
            }
        }
        (Some(GraphDrag::Detach(k, tip)), Some(p)) if k < graph.edges.len() => {
            target = node_at(&graph.nodes, p).map(|(j, _)| j);
            if !down {
                // Released on a node: that end attaches there; elsewhere the edge goes.
                match target {
                    Some(j) => {
                        let id = graph.nodes[j].id.clone();
                        let edge = &mut graph.edges[k];
                        if tip {
                            edge.to = id;
                        } else {
                            edge.from = id;
                        }
                    }
                    None => {
                        graph.edges.remove(k);
                        edit.chosen = None;
                    }
                }
                edit.drag = None;
                target = None;
            }
        }
        (Some(_), _) if !down => edit.drag = None,
        _ => {}
    }
    if edit.drag.is_some() {
        ui.ctx().request_repaint();
    }
    let typing = ui.ctx().egui_wants_keyboard_input();
    if keys
        && !typing
        && edit.label.is_none()
        && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::X))
    {
        match edit.chosen.take() {
            Some(Chosen::Node(i)) if i < graph.nodes.len() => graph.remove_node(i),
            Some(Chosen::Edge(k)) if k < graph.edges.len() => {
                graph.edges.remove(k);
            }
            _ => {}
        }
        edit.drag = None;
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
    let draw_edge = |shape: &EdgeShape, width: f32, color: egui::Color32| {
        painter.add(egui::Shape::line(
            shape.line.clone(),
            Stroke::new(width, color),
        ));
        arrowhead(&painter, shape.tip, shape.dir, color);
    };
    let detached = match edit.drag {
        Some(GraphDrag::Detach(k, tip)) => Some((k, tip)),
        _ => None,
    };
    for (k, shape) in shapes.iter().enumerate() {
        if detached.is_some_and(|(d, _)| d == k) {
            continue;
        }
        if let Some(shape) = shape {
            if edit.chosen == Some(Chosen::Edge(k)) {
                draw_edge(shape, 2.5, ACCENT);
            } else {
                draw_edge(shape, 1.5, TEXT_WEAK);
            }
        }
    }
    if let (Some(GraphDrag::Connect(i, _)), Some(p)) = (edit.drag, pointer)
        && let Some(n) = graph.nodes.get(i)
    {
        let c = center(n);
        match target {
            // The edge as it will be: the next of its ends.
            Some(j) => draw_edge(&planned(graph, i, j, None), 2.5, ACCENT),
            None if (p - c).length() > r => {
                let dir = (p - c).normalized();
                let color = ACCENT.gamma_multiply(0.6);
                painter.line_segment(
                    [c + dir * r, p - dir * ARROW * 0.8],
                    Stroke::new(1.5, color),
                );
                arrowhead(&painter, p, dir, color);
            }
            None => {}
        }
    }
    // A detached edge: from its other end to the pointer, or as it will be on a target.
    if let (Some((k, tip)), Some(p)) = (detached, pointer)
        && let Some(e) = graph.edges.get(k)
        && let Some(fixed) = graph.node_index(if tip { &e.from } else { &e.to })
    {
        let c = center(&graph.nodes[fixed]);
        let color = ACCENT.gamma_multiply(0.6);
        match (target, tip) {
            (Some(j), true) => draw_edge(&planned(graph, fixed, j, Some(k)), 2.5, ACCENT),
            (Some(j), false) => draw_edge(&planned(graph, j, fixed, Some(k)), 2.5, ACCENT),
            (None, true) => {
                let dir = (p - c).normalized();
                painter.line_segment(
                    [c + dir * r, p - dir * ARROW * 0.8],
                    Stroke::new(1.5, color),
                );
                arrowhead(&painter, p, dir, color);
            }
            (None, false) => {
                let dir = (c - p).normalized();
                let end = c - dir * r;
                painter.line_segment([p, end - dir * ARROW * 0.8], Stroke::new(1.5, color));
                arrowhead(&painter, end, dir, color);
            }
        }
    }
    // An edge end under the pointer, grabbed before rims.
    let hovered_end = match (edit.drag, pointer) {
        (None, Some(p)) if response.hovered() => end_at(&shapes, p),
        _ => None,
    };
    let end_dot = hovered_end.and_then(|(k, tip)| {
        let shape = shapes[k].as_ref()?;
        Some(if tip { shape.tip } else { shape.line[0] })
    });
    if end_dot.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
    if detached.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    let hovered_rim = match (edit.drag, pointer) {
        (None, Some(p)) if response.hovered() && hovered_end.is_none() => node_at(&graph.nodes, p)
            .filter(|&(_, d)| d >= BODY * r)
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
        if target == Some(i) {
            painter.circle_filled(c, r, ACCENT.gamma_multiply(0.15));
        }
        let stroke = if edit.chosen == Some(Chosen::Node(i)) {
            Stroke::new(2.0, ACCENT)
        } else {
            Stroke::new(1.0, STROKE)
        };
        painter.circle_stroke(c, r - stroke.width / 2.0, stroke);
        if hovered_rim == Some(i) && edit.drag.is_none() {
            // The rim's band, from the body out to RIM past the border.
            let (inner, outer) = (BODY * r, r + RIM);
            painter.circle_stroke(
                c,
                (inner + outer) / 2.0,
                Stroke::new(outer - inner, ACCENT.gamma_multiply(0.15)),
            );
        }
        if hovered_rim == Some(i) || target == Some(i) {
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
    if let Some(at) = end_dot {
        painter.circle_filled(at, 4.0, ACCENT);
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
         drag a node's rim onto a node to connect · Delete or X removes · Shift snaps to 0.25",
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
