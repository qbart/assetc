//! The `.agraph` editor: the canvas grid of nodes and edges and the Inspector's Graph tab;
//! see docs/features/graphs.md.

use crate::agraph::{Edge, Node};
use crate::atree;
use crate::graph_layout::{self, Algorithm, Align as AlignNodes, Direction};
use crate::theme::{BG_HOVER, CHANNEL_R};

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
/// Undo steps kept per file.
const HISTORY: usize = 100;
/// The Layout panel's width, in points.
const PANEL: f32 = 260.0;

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
    /// The selected nodes, in file order; the chosen node is one of them. See
    /// docs/features/graph-layout.md#selection.
    selected: Vec<usize>,
    drag: Option<GraphDrag>,
    /// `edit` is an undone or redone version, not a new step.
    replay: bool,
    /// A node's or edge's label being edited on the canvas, and whether its field still
    /// needs focus.
    label: Option<(Chosen, String, bool)>,
    /// A tree (`.atree`): node 0 is the root and the edges link parents to children; see
    /// docs/features/trees.md.
    pub(super) tree: bool,
}

impl GraphEdit {
    /// What the canvas has chosen, if it is still there.
    pub(super) fn chosen(&self) -> Option<Chosen> {
        self.chosen.filter(|c| match *c {
            Chosen::Node(i) => i < self.edit.nodes.len(),
            Chosen::Edge(k) => k < self.edit.edges.len(),
        })
    }

    /// `graph` as read (a tree when `tree`), keeping `previous`'s choice.
    pub(super) fn new(graph: Graph, tree: bool, previous: GraphEdit) -> Self {
        // The chosen node by id: a tree is read back depth first, so its places change.
        let (chosen, selected) = follow(&previous, &graph);
        Self {
            edit: graph.clone(),
            saved: graph,
            tree,
            chosen,
            selected,
            ..previous
        }
    }

    /// The selection holds the chosen node, and nothing when no node is chosen.
    fn normalize(&mut self) {
        let n = self.edit.nodes.len();
        self.selected.retain(|&i| i < n);
        self.selected.sort_unstable();
        self.selected.dedup();
        match self.chosen() {
            Some(Chosen::Node(i)) if !self.selected.contains(&i) => self.selected = vec![i],
            Some(Chosen::Node(_)) => {}
            _ => self.selected.clear(),
        }
    }

    /// Chooses and selects only node `i`.
    fn choose_only(&mut self, i: usize) {
        self.chosen = Some(Chosen::Node(i));
        self.selected = vec![i];
    }

    /// What the toolbar's Snap and the Layout panel move: the selection when it has 2+
    /// nodes, else every node (empty).
    fn targets(&self) -> Vec<usize> {
        if self.selected.len() >= 2 {
            self.selected.clone()
        } else {
            Vec::new()
        }
    }

    /// Removes the selected nodes (a tree's with what is under them, never its root), else
    /// the chosen edge.
    fn remove_chosen(&mut self) {
        let chosen = self.chosen();
        let graph = &mut self.edit;
        match chosen {
            Some(Chosen::Node(_)) => {
                let ids: Vec<String> = self
                    .selected
                    .iter()
                    .map(|&i| graph.nodes[i].id.clone())
                    .collect();
                for id in ids {
                    match graph.node_index(&id) {
                        Some(0) if self.tree => {}
                        Some(i) if self.tree => atree::remove(graph, i),
                        Some(i) => graph.remove_node(i),
                        None => {}
                    }
                }
                // A tree keeps its root, chosen when it was.
                self.chosen = (self.tree && self.selected.contains(&0)).then_some(Chosen::Node(0));
            }
            Some(Chosen::Edge(k)) => {
                graph.edges.remove(k);
                self.chosen = None;
            }
            None => {}
        }
        self.selected.clear();
        self.label = None;
        self.drag = None;
    }
}

/// `previous`'s choice and selection in `graph`, nodes followed by id.
fn follow(previous: &GraphEdit, graph: &Graph) -> (Option<Chosen>, Vec<usize>) {
    let find = |i: usize| {
        previous
            .edit
            .nodes
            .get(i)
            .and_then(|n| graph.node_index(&n.id))
    };
    let chosen = match previous.chosen {
        Some(Chosen::Node(i)) => find(i).map(Chosen::Node),
        Some(Chosen::Edge(k)) => (k < graph.edges.len()).then_some(Chosen::Edge(k)),
        None => None,
    };
    let mut selected: Vec<usize> = previous.selected.iter().filter_map(|&i| find(i)).collect();
    selected.sort_unstable();
    (chosen, selected)
}

/// Undo and redo, per file for the session; see docs/features/graph-layout.md#undo.
#[derive(Default)]
pub(super) struct History {
    undo: Vec<Graph>,
    redo: Vec<Graph>,
}

impl History {
    /// Puts the graph back as it was before the last step (`back`), or after the next.
    fn step(&mut self, edit: &mut GraphEdit, back: bool) {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let Some(graph) = from.pop() else { return };
        to.push(edit.edit.clone());
        let (chosen, selected) = follow(edit, &graph);
        edit.edit = graph;
        edit.chosen = chosen;
        edit.selected = selected;
        edit.label = None;
        edit.drag = None;
        edit.replay = edit.edit != edit.saved;
        edit.normalize();
    }
}

/// A graph file's state for the session: its view, undo history and layout options.
#[derive(Default)]
pub(super) struct GraphSession {
    pub(super) view: GraphView,
    pub(super) history: History,
    layout: graph_layout::Options,
    /// The Layout panel is open.
    panel: bool,
}

#[derive(Clone, Copy)]
enum GraphDrag {
    /// A node (and the others selected), held this far (in units) from its center, and
    /// whether it has moved.
    Move(usize, (f64, f64), bool),
    /// A selection box from this point (in units), adding to the selection or not.
    Select((f64, f64), bool),
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

/// The text to write once nothing is being dragged or typed into; each write is an undo
/// step, unless it is an undone or redone version.
pub(super) fn graph_commit(ui: &Ui, edit: &mut GraphEdit, history: &mut History) -> Option<String> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        if !std::mem::take(&mut edit.replay) {
            history.undo.push(edit.saved.clone());
            if history.undo.len() > HISTORY {
                history.undo.remove(0);
            }
            history.redo.clear();
        }
        edit.saved = edit.edit.clone();
        if edit.tree {
            atree::to_json(&edit.edit)
        } else {
            edit.edit.to_json()
        }
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
    sessions: &mut HashMap<PathBuf, GraphSession>,
    keys: bool,
) -> Option<String> {
    let session = sessions.entry(inspected.path.clone()).or_default();
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
    edit.normalize();
    let view = &mut session.view;
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

    // The edge whose curve is nearest a point, within reach.
    let edge_at = |shapes: &[Option<EdgeShape>], p: egui::Pos2| {
        shapes
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
            .map(|(k, _)| k)
    };

    // Choose, move, connect, detach, add, rename and remove; a tree's links are not edges
    // to choose, see docs/features/trees.md#canvas.
    let tree = edit.tree;
    let graph = &mut edit.edit;
    if response.double_clicked()
        && let Some(p) = pointer
    {
        if let Some((i, _)) = node_at(&graph.nodes, p) {
            edit.label = Some((Chosen::Node(i), graph.nodes[i].label.clone(), true));
            edit.chosen = Some(Chosen::Node(i));
        } else if tree {
            let parent = match edit.chosen {
                Some(Chosen::Node(i)) if i < graph.nodes.len() => i,
                _ => 0,
            };
            let (x, y) = to_units(p);
            let child = atree::add_child(graph, parent, place(x, shift), place(y, shift));
            edit.chosen = Some(Chosen::Node(child));
        } else if let Some(k) = edge_at(&shapes(graph), p) {
            edit.label = Some((Chosen::Edge(k), graph.edges[k].label.clone(), true));
            edit.chosen = Some(Chosen::Edge(k));
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
        let add = ui.input(|i| i.modifiers.command);
        let under = node_at(&graph.nodes, p);
        if add {
            // Ctrl: a node in or out of the selection, or a box adding to it.
            edit.drag = match under {
                Some((i, _)) => {
                    if let Some(at) = edit.selected.iter().position(|&j| j == i) {
                        edit.selected.remove(at);
                        if edit.chosen == Some(Chosen::Node(i)) {
                            edit.chosen = edit.selected.first().copied().map(Chosen::Node);
                        }
                    } else {
                        if !matches!(edit.chosen, Some(Chosen::Node(_))) {
                            edit.selected.clear();
                        }
                        edit.selected.push(i);
                        edit.selected.sort_unstable();
                        edit.chosen = Some(Chosen::Node(i));
                    }
                    None
                }
                None => Some(GraphDrag::Select(to_units(p), true)),
            };
        } else if let Some((k, tip)) = end_at(&shapes(graph), p).filter(|_| !tree) {
            edit.drag = Some(GraphDrag::Detach(k, tip));
            edit.chosen = Some(Chosen::Edge(k));
        } else if let Some((i, d)) = under {
            if d >= BODY * r {
                edit.drag = Some(GraphDrag::Connect(i, false));
                edit.chosen = Some(Chosen::Node(i));
                edit.selected = vec![i];
            } else {
                let (x, y) = to_units(p);
                let n = &graph.nodes[i];
                edit.drag = Some(GraphDrag::Move(i, (x - n.x, y - n.y), false));
                // Pressing a selected node keeps the selection, to move it all.
                if !edit.selected.contains(&i) {
                    edit.selected = vec![i];
                }
                edit.chosen = Some(Chosen::Node(i));
            }
        } else if let Some(k) = edge_at(&shapes(graph), p).filter(|_| !tree) {
            edit.chosen = Some(Chosen::Edge(k));
            edit.drag = None;
        } else {
            edit.drag = Some(GraphDrag::Select(to_units(p), false));
        }
    }
    edit.normalize();
    let graph = &mut edit.edit;
    let down = ui.input(|i| i.pointer.primary_down());
    // The node a rubber edge would end on: any node, its start once the pointer has left it.
    let mut target = None;
    match (edit.drag, pointer) {
        (Some(GraphDrag::Move(i, (ox, oy), moved)), Some(p)) if down && i < graph.nodes.len() => {
            // The other selected nodes keep their offsets from this one.
            let (x, y) = to_units(p);
            let (nx, ny) = (place(x - ox, shift), place(y - oy, shift));
            let (dx, dy) = (nx - graph.nodes[i].x, ny - graph.nodes[i].y);
            if dx != 0.0 || dy != 0.0 {
                for &j in edit.selected.iter().filter(|&&j| j != i) {
                    let n = &mut graph.nodes[j];
                    n.x = place(n.x + dx, false);
                    n.y = place(n.y + dy, false);
                }
                graph.nodes[i].x = nx;
                graph.nodes[i].y = ny;
            }
            edit.drag = Some(GraphDrag::Move(
                i,
                (ox, oy),
                moved || dx != 0.0 || dy != 0.0,
            ));
        }
        (Some(GraphDrag::Move(i, _, moved)), _) if !down => {
            // A press on a selected node without moving selects only it.
            if !moved && i < graph.nodes.len() {
                edit.choose_only(i);
            }
            edit.drag = None;
        }
        (Some(GraphDrag::Select(from, add)), Some(p)) if !down => {
            let start = to_screen(from.0, from.1);
            if (p - start).length() < 4.0 {
                // A click: nothing chosen, unless adding.
                if !add {
                    edit.chosen = None;
                }
            } else {
                let area = Rect::from_two_pos(start, p);
                let inside: Vec<usize> = (0..graph.nodes.len())
                    .filter(|&i| area.contains(center(&graph.nodes[i])))
                    .collect();
                if !add {
                    edit.selected.clear();
                }
                edit.selected.extend(inside);
                edit.selected.sort_unstable();
                edit.selected.dedup();
                let kept =
                    matches!(edit.chosen, Some(Chosen::Node(i)) if edit.selected.contains(&i));
                if !kept {
                    edit.chosen = edit.selected.first().copied().map(Chosen::Node);
                }
            }
            edit.drag = None;
            edit.normalize();
        }
        (Some(GraphDrag::Connect(i, left)), Some(p)) if i < graph.nodes.len() => {
            let under = node_at(&graph.nodes, p).map(|(j, _)| j);
            let left = left || under != Some(i);
            target = match tree {
                // Not the root, nor this node or one above it: that would make a loop.
                true => under.filter(|&j| j != 0 && !atree::is_ancestor(graph, j, i)),
                false => under.filter(|&j| j != i || left),
            };
            if down {
                edit.drag = Some(GraphDrag::Connect(i, left));
            } else {
                match (tree, target) {
                    // Released on a target: a new edge to it, or it moves under this node.
                    (false, Some(j)) => {
                        let (from, to) = (graph.nodes[i].id.clone(), graph.nodes[j].id.clone());
                        graph.edges.push(Edge::new(from, to));
                        edit.chosen = Some(Chosen::Edge(graph.edges.len() - 1));
                    }
                    (true, Some(j)) => {
                        atree::move_under(graph, j, i);
                        edit.chosen = Some(Chosen::Node(j));
                    }
                    // A tree's rim dropped on empty grid: a child there.
                    (true, None) if under.is_none() => {
                        let (x, y) = to_units(p);
                        let child = atree::add_child(graph, i, place(x, shift), place(y, shift));
                        edit.chosen = Some(Chosen::Node(child));
                    }
                    _ => {}
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
    if keys && !typing && edit.label.is_none() {
        let (command, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |key| ui.input(|i| i.key_pressed(key));
        if command && pressed(egui::Key::Z) {
            session.history.step(edit, !shift);
        } else if command && pressed(egui::Key::Y) {
            session.history.step(edit, false);
        } else if command && pressed(egui::Key::A) {
            edit.selected = (0..edit.edit.nodes.len()).collect();
            if !matches!(edit.chosen(), Some(Chosen::Node(_))) {
                edit.chosen = (!edit.selected.is_empty()).then_some(Chosen::Node(0));
            }
        } else if pressed(egui::Key::Escape) {
            if session.panel {
                session.panel = false;
            } else {
                edit.selected.clear();
                edit.normalize();
            }
        } else if !command && (pressed(egui::Key::Delete) || pressed(egui::Key::X)) {
            edit.remove_chosen();
        }
    }
    let graph = &mut edit.edit;

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
    // A tree's links have no arrowhead: they reach the child's border.
    let draw_edge = |shape: &EdgeShape, width: f32, color: egui::Color32| {
        let mut line = shape.line.clone();
        if tree {
            line.push(shape.tip);
        }
        painter.add(egui::Shape::line(line, Stroke::new(width, color)));
        if !tree {
            arrowhead(&painter, shape.tip, shape.dir, color);
        }
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
                let end = if tree { p } else { p - dir * ARROW * 0.8 };
                painter.line_segment([c + dir * r, end], Stroke::new(1.5, color));
                if !tree {
                    arrowhead(&painter, p, dir, color);
                }
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
        (None, Some(p)) if response.hovered() && !tree => end_at(&shapes, p),
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
    // The node under the pointer, or the one being moved.
    let hovered_node = match (edit.drag, pointer) {
        (Some(GraphDrag::Move(i, _, _)), _) => Some(i),
        (None, Some(p)) if response.hovered() && end_dot.is_none() => {
            node_at(&graph.nodes, p).map(|(i, _)| i)
        }
        _ => None,
    };
    for (i, n) in graph.nodes.iter().enumerate() {
        let c = center(n);
        let fill = if hovered_node == Some(i) {
            BG_HOVER
        } else {
            BG_HEADER
        };
        painter.circle_filled(c, r, fill);
        // A node's color, see docs/features/graphs.md#node-colors.
        let color = n.color.as_deref().and_then(hex_color32);
        if let Some(color) = color {
            painter.circle_filled(c, r, color.gamma_multiply(0.35));
        }
        if target == Some(i) {
            painter.circle_filled(c, r, ACCENT.gamma_multiply(0.15));
        }
        let stroke = match (edit.chosen == Some(Chosen::Node(i)), color) {
            (true, _) => Stroke::new(2.0, ACCENT),
            (false, _) if edit.selected.contains(&i) => {
                Stroke::new(2.0, ACCENT.gamma_multiply(0.55))
            }
            // The root of a tree.
            (false, _) if tree && i == 0 => Stroke::new(2.0, TEXT_WEAK),
            (false, Some(color)) => Stroke::new(1.0, color),
            (false, None) => Stroke::new(1.0, STROKE),
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
        if edit
            .label
            .as_ref()
            .is_none_or(|(c, _, _)| *c != Chosen::Node(i))
        {
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
    if size >= 6.0 && !tree {
        for (k, (e, shape)) in graph.edges.iter().zip(&shapes).enumerate() {
            let editing = edit
                .label
                .as_ref()
                .is_some_and(|(c, _, _)| *c == Chosen::Edge(k));
            let Some(shape) = shape
                .as_ref()
                .filter(|_| !editing && detached.is_none_or(|(d, _)| d != k))
            else {
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
        if tree {
            "Double-click to add a child · double-click a node to rename · drag a node's rim \
             to add a child or onto a node to move it under · drag on the grid or Ctrl+click to \
             select · Delete or X removes · Shift snaps to 0.25"
        } else {
            "Double-click to add a node · double-click a node or edge to rename · \
             drag a node's rim onto a node to connect · drag on the grid or Ctrl+click to \
             select · Delete or X removes · Shift snaps to 0.25"
        },
        FontId::proportional(12.0),
        TEXT_WEAK,
    );

    // The label being edited, over its node or the middle of its edge.
    if let Some((chosen, text, focus)) = &mut edit.label {
        let at = match *chosen {
            Chosen::Node(i) => graph
                .nodes
                .get(i)
                .map(|n| Rect::from_center_size(center(n), vec2((2.0 * r).max(120.0), 24.0))),
            Chosen::Edge(k) => shapes
                .get(k)
                .and_then(Option::as_ref)
                .map(|shape| Rect::from_center_size(shape.middle, vec2(120.0, 24.0))),
        };
        if let Some(at) = at {
            let field = egui::TextEdit::singleline(text)
                .id(ui.id().with("graph_label"))
                .horizontal_align(Align::Center);
            let response = ui.put(at, field);
            if *focus {
                response.request_focus();
                *focus = false;
            } else if !response.has_focus() {
                if !ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    let label = match *chosen {
                        Chosen::Node(i) => &mut graph.nodes[i].label,
                        Chosen::Edge(k) => &mut graph.edges[k].label,
                    };
                    *label = text.clone();
                }
                edit.label = None;
            }
        } else {
            edit.label = None;
        }
    }
    // The selection box.
    if let (Some(GraphDrag::Select(from, _)), Some(p)) = (edit.drag, pointer) {
        let area = Rect::from_two_pos(to_screen(from.0, from.1), p);
        painter.rect(
            area,
            0,
            ACCENT.gamma_multiply(0.1),
            Stroke::new(1.0, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    graph_toolbar(ui, rect, edit, session);
    graph_commit(ui, edit, &mut session.history)
}

/// The toolbar's groups, in order; see docs/features/graph-layout.md#toolbar.
const HISTORY_GROUP: usize = 0;
const ALIGN_GROUP: usize = 1;
const ARRANGE_GROUP: usize = 2;
const LAYOUT_GROUP: usize = 3;

/// The toolbar and the Layout panel; see docs/features/graph-layout.md#toolbar.
fn graph_toolbar(ui: &mut Ui, rect: Rect, edit: &mut GraphEdit, session: &mut GraphSession) {
    let selected = edit.selected.len();
    let has_nodes = !edit.edit.nodes.is_empty();
    let tool = |icon: Icon, name: &str, on: bool, enabled: bool| Tool {
        glyph: Glyph::Icon(icon),
        name: name.to_owned(),
        on,
        enabled,
    };
    let history = [
        tool(Icon::Undo2, "Undo", false, !session.history.undo.is_empty()),
        tool(Icon::Redo2, "Redo", false, !session.history.redo.is_empty()),
    ];
    let aligns = [
        (Icon::AlignStartVertical, "Align left"),
        (Icon::AlignCenterVertical, "Align horizontal centers"),
        (Icon::AlignEndVertical, "Align right"),
        (Icon::AlignStartHorizontal, "Align top"),
        (Icon::AlignCenterHorizontal, "Align vertical centers"),
        (Icon::AlignEndHorizontal, "Align bottom"),
    ]
    .map(|(icon, name)| tool(icon, name, false, selected >= 2));
    let arrange = [
        tool(
            Icon::AlignHorizontalDistributeCenter,
            "Distribute horizontally",
            false,
            selected >= 3,
        ),
        tool(
            Icon::AlignVerticalDistributeCenter,
            "Distribute vertically",
            false,
            selected >= 3,
        ),
        tool(Icon::Magnet, "Snap to grid", false, has_nodes),
    ];
    let layout = [tool(Icon::Network, "Layout", session.panel, has_nodes)];
    let groups: [&[Tool]; 4] = [&history, &aligns, &arrange, &layout];
    match toolbar(ui, rect, &groups) {
        Some((HISTORY_GROUP, button)) => session.history.step(edit, button == 0),
        Some((ALIGN_GROUP, button)) => {
            let how = [
                AlignNodes::Left,
                AlignNodes::HorizontalCenters,
                AlignNodes::Right,
                AlignNodes::Top,
                AlignNodes::VerticalCenters,
                AlignNodes::Bottom,
            ][button];
            graph_layout::align(&mut edit.edit, &edit.selected, how);
        }
        Some((ARRANGE_GROUP, 2)) => {
            let nodes = match edit.targets() {
                all if all.is_empty() => (0..edit.edit.nodes.len()).collect(),
                some => some,
            };
            graph_layout::snap(&mut edit.edit, &nodes);
        }
        Some((ARRANGE_GROUP, button)) => {
            graph_layout::distribute(&mut edit.edit, &edit.selected, button == 0);
        }
        Some((LAYOUT_GROUP, _)) => session.panel = !session.panel,
        _ => {}
    }
    if session.panel && has_nodes {
        // Beside the Layout button: the toolbar's 8 px margin, 11 buttons and 3 gaps down.
        let top = rect.top() + 8.0 + 11.0 * 32.0 + 3.0 * 8.0;
        let left = rect.right() - 8.0 - 36.0 - 8.0 - PANEL;
        layout_panel(ui, pos2(left, top), edit, &mut session.layout);
    }
}

/// The Layout panel; see docs/features/graph-layout.md#layout.
fn layout_panel(
    ui: &mut Ui,
    at: egui::Pos2,
    edit: &mut GraphEdit,
    options: &mut graph_layout::Options,
) {
    let tree = edit.tree;
    egui::Area::new(ui.id().with("graph_layout_panel"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(BG_PANEL)
                .stroke(Stroke::new(1.0, STROKE))
                .corner_radius(6)
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(PANEL - 16.0);
                    property_style(ui);
                    prop_row(ui, "Layout", |ui, width| {
                        egui::ComboBox::from_id_salt("graph_layout_algorithm")
                            .width(width - 8.0)
                            .selected_text(options.algorithm.name(tree))
                            .show_ui(ui, |ui| {
                                for a in Algorithm::ALL {
                                    ui.selectable_value(&mut options.algorithm, a, a.name(tree));
                                }
                            });
                    });
                    let algorithm = options.algorithm;
                    if algorithm == Algorithm::Layered {
                        prop_row(ui, "Direction", |ui, width| {
                            egui::ComboBox::from_id_salt("graph_layout_direction")
                                .width(width - 8.0)
                                .selected_text(options.direction.name())
                                .show_ui(ui, |ui| {
                                    for d in Direction::ALL {
                                        ui.selectable_value(&mut options.direction, d, d.name());
                                    }
                                });
                        });
                    }
                    let spacing = |ui: &mut Ui, label: &str, value: &mut f64| {
                        prop_row(ui, label, |ui, width| {
                            let field = egui::DragValue::new(value)
                                .range(0.5..=10.0)
                                .speed(0.05)
                                .fixed_decimals(2);
                            ui.add_sized(vec2(width, 22.0), field);
                        });
                        *value = ((*value * 4.0).round() / 4.0).clamp(0.5, 10.0);
                    };
                    spacing(ui, "Node spacing", &mut options.node_spacing);
                    let layered = algorithm == Algorithm::Layered;
                    if layered || (algorithm == Algorithm::Circle && tree) {
                        spacing(ui, "Layer spacing", &mut options.layer_spacing);
                    }
                    if algorithm == Algorithm::Grid {
                        prop_row(ui, "Columns", |ui, width| {
                            let field = egui::DragValue::new(&mut options.columns)
                                .range(0..=100)
                                .speed(0.1);
                            ui.add_sized(vec2(width, 22.0), field);
                        });
                    }
                    ui.add_space(6.0);
                    let targets = edit.targets();
                    let what = if targets.is_empty() {
                        format!("All {} nodes", edit.edit.nodes.len())
                    } else {
                        format!("The {} selected nodes", targets.len())
                    };
                    ui.label(RichText::new(what).size(12.0).color(TEXT_WEAK));
                    ui.add_space(4.0);
                    let apply = egui::Button::new(RichText::new("Apply").color(BG_BAR))
                        .fill(ACCENT)
                        .min_size(vec2(ui.available_width(), 26.0));
                    if ui.add(apply).clicked() {
                        graph_layout::layout(&mut edit.edit, tree, &targets, options);
                    }
                });
        });
}

/// The chosen node or edge, or the node list; returns the graph to write. See
/// docs/features/graphs.md#graph-tab.
pub(super) fn graph_tab(
    ui: &mut Ui,
    inspected: &mut Inspected,
    session: &mut GraphSession,
) -> Option<String> {
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
    edit.chosen = edit.chosen();
    edit.normalize();
    let view = &mut session.view;
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
                    Some(Chosen::Node(_)) if edit.selected.len() >= 2 => {
                        several_nodes(ui, edit);
                        problems(ui);
                    }
                    None => {
                        problems(ui);
                        if edit.tree {
                            tree_outline(ui, edit, view);
                        } else {
                            node_list(ui, edit, view);
                        }
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
    edit.normalize();
    graph_commit(ui, edit, &mut session.history)
}

/// Several selected nodes: Delete, one Color for all, and a row each; see
/// docs/features/graph-layout.md#selection.
fn several_nodes(ui: &mut Ui, edit: &mut GraphEdit) {
    let count = edit.selected.len();
    if pane_header(ui, &format!("{count} nodes"), "Delete nodes", true) {
        edit.remove_chosen();
        return;
    }
    let first = edit.selected[0];
    let mut color = edit.edit.nodes[first].color.clone();
    let before = color.clone();
    color_row(ui, &mut color, true);
    if color != before {
        for &i in &edit.selected {
            edit.edit.nodes[i].color = color.clone();
        }
    }
    ui.add_space(8.0);
    let mut only = None;
    for &i in &edit.selected {
        let n = &edit.edit.nodes[i];
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
            only = Some(i);
        }
    }
    if let Some(i) = only {
        edit.choose_only(i);
    }
}

/// A tree's outline, depth first and indented, to choose a node, and Add child; see
/// docs/features/trees.md#tree-tab.
fn tree_outline(ui: &mut Ui, edit: &mut GraphEdit, view: &mut GraphView) {
    let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add child");
    if ui.add(add).clicked() && !edit.edit.nodes.is_empty() {
        let (x, y) = view.center;
        let child = atree::add_child(&mut edit.edit, 0, place(x, true), place(y, true));
        edit.chosen = Some(Chosen::Node(child));
        return;
    }
    ui.add_space(8.0);
    let mut rows = Vec::new();
    let mut stack = vec![(0usize, 0usize)];
    while let Some((i, depth)) = stack.pop() {
        if i >= edit.edit.nodes.len() || rows.iter().any(|(r, _)| *r == i) {
            continue;
        }
        rows.push((i, depth));
        for c in atree::children(&edit.edit, i).into_iter().rev() {
            stack.push((c, depth + 1));
        }
    }
    for (i, depth) in rows {
        let n = &edit.edit.nodes[i];
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 4, BG_HEADER);
        }
        ui.painter().text(
            rect.left_center() + vec2(6.0 + 16.0 * depth as f32, 0.0),
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
fn pane_header(ui: &mut Ui, title: &str, tooltip: &str, enabled: bool) -> bool {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(14.0).color(TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            icon_button(ui, Icon::Trash2, tooltip, enabled).clicked()
        })
        .inner
    })
    .inner
}

fn node_fields(ui: &mut Ui, edit: &mut GraphEdit, i: usize) {
    let tree = edit.tree;
    let graph = &mut edit.edit;
    // A tree's root stays; removing a node there removes what is under it.
    if pane_header(ui, "Node", "Delete node", !(tree && i == 0)) {
        edit.remove_chosen();
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
    color_row(ui, &mut node.color, true);
    for (label, value) in [("X", &mut node.x), ("Y", &mut node.y)] {
        prop_row(ui, label, |ui, width| {
            let field = egui::DragValue::new(value).speed(0.01).fixed_decimals(3);
            ui.add_sized(vec2(width, 22.0), field);
        });
    }
    if tree {
        tree_parent(ui, edit, i);
        let graph = &mut edit.edit;
        let node = &mut graph.nodes[i];
        ui.add_space(6.0);
        ui.label(RichText::new("Data").size(13.0).color(TEXT_WEAK));
        properties_editor(ui, &mut node.data);
        ui.add_space(6.0);
        ui.label(RichText::new("Children").size(13.0).color(TEXT_WEAK));
        for c in atree::children(graph, i) {
            let n = &graph.nodes[c];
            let text = if n.label.is_empty() {
                n.id.clone()
            } else {
                format!("{}  {}", n.id, n.label)
            };
            if ui
                .add(egui::Label::new(RichText::new(text).monospace()).sense(Sense::click()))
                .clicked()
            {
                edit.chosen = Some(Chosen::Node(c));
            }
        }
        let add = egui::Button::image_and_text(Icon::Plus.image(14.0, TEXT), "Add child");
        if ui.add(add).clicked() {
            let (x, y) = (graph.nodes[i].x, graph.nodes[i].y + 1.0);
            let child = atree::add_child(graph, i, x, y);
            edit.chosen = Some(Chosen::Node(child));
        }
        return;
    }
    let graph = &mut edit.edit;
    let node = &mut graph.nodes[i];
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

/// A tree node's Parent: every node it can move under; the root has none.
fn tree_parent(ui: &mut Ui, edit: &mut GraphEdit, i: usize) {
    let graph = &mut edit.edit;
    let current = atree::parent(graph, i);
    let shown = current.map_or("—".to_owned(), |p| graph.nodes[p].id.clone());
    let mut chosen = None;
    prop_row(ui, "Parent", |ui, width| {
        ui.add_enabled_ui(i != 0, |ui| {
            egui::ComboBox::from_id_salt("tree_parent")
                .width(width - 8.0)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (p, n) in graph.nodes.iter().enumerate() {
                        // Not itself nor anything under it.
                        if atree::is_ancestor(graph, i, p) {
                            continue;
                        }
                        if ui.selectable_label(current == Some(p), &n.id).clicked() {
                            chosen = Some(p);
                        }
                    }
                });
        });
    });
    if let Some(p) = chosen.filter(|p| Some(*p) != current) {
        atree::move_under(graph, i, p);
    }
}

fn edge_fields(ui: &mut Ui, edit: &mut GraphEdit, k: usize) {
    let graph = &mut edit.edit;
    if pane_header(ui, "Edge", "Delete edge", true) {
        graph.edges.remove(k);
        edit.chosen = None;
        edit.label = None;
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
