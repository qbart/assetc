//! Aligning and laying out the nodes of a graph or tree; see docs/features/graph-layout.md.

use std::f64::consts::PI;

use crate::agraph::Graph;
use crate::atree;

/// The snapping step, in units.
const SNAP: f64 = 0.25;
/// Force layout: steps, and the pull toward the middle.
const FORCE_STEPS: usize = 300;
const GRAVITY: f64 = 0.1;

/// A layout; trees call Layered "Tidy tree" and Circle "Radial".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Algorithm {
    #[default]
    Layered,
    Circle,
    Force,
    Grid,
}

impl Algorithm {
    pub const ALL: [Self; 4] = [Self::Layered, Self::Circle, Self::Force, Self::Grid];

    pub fn name(self, tree: bool) -> &'static str {
        match (self, tree) {
            (Self::Layered, false) => "Layered",
            (Self::Layered, true) => "Tidy tree",
            (Self::Circle, false) => "Circle",
            (Self::Circle, true) => "Radial",
            (Self::Force, _) => "Force",
            (Self::Grid, _) => "Grid",
        }
    }
}

/// Where the layers of a layered or tidy layout go.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    #[default]
    Down,
    Right,
    Up,
    Left,
}

impl Direction {
    pub const ALL: [Self; 4] = [Self::Down, Self::Right, Self::Up, Self::Left];

    pub fn name(self) -> &'static str {
        match self {
            Self::Down => "Down",
            Self::Right => "Right",
            Self::Up => "Up",
            Self::Left => "Left",
        }
    }

    /// A point `across` the layers and `along` them, as x and y.
    fn point(self, across: f64, along: f64) -> (f64, f64) {
        match self {
            Self::Down => (across, along),
            Self::Up => (across, -along),
            Self::Right => (along, across),
            Self::Left => (-along, across),
        }
    }
}

/// The Layout panel's options; see docs/features/graph-layout.md#layout.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Options {
    pub algorithm: Algorithm,
    pub direction: Direction,
    pub node_spacing: f64,
    pub layer_spacing: f64,
    /// Grid: `0` is `ceil(√n)`.
    pub columns: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::Layered,
            direction: Direction::Down,
            node_spacing: 1.5,
            layer_spacing: 2.0,
            columns: 0,
        }
    }
}

/// Rounded to 3 decimals, as positions are written.
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// How the Align buttons move centers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    HorizontalCenters,
    Right,
    Top,
    VerticalCenters,
    Bottom,
}

/// Aligns the centers of `nodes`.
pub fn align(graph: &mut Graph, nodes: &[usize], how: Align) {
    let xs = nodes.iter().map(|&i| graph.nodes[i].x);
    let ys = nodes.iter().map(|&i| graph.nodes[i].y);
    let (lo_x, hi_x) = xs.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
    let (lo_y, hi_y) = ys.fold((f64::MAX, f64::MIN), |(l, h), v| (l.min(v), h.max(v)));
    for &i in nodes {
        let n = &mut graph.nodes[i];
        match how {
            Align::Left => n.x = lo_x,
            Align::HorizontalCenters => n.x = round3((lo_x + hi_x) / 2.0),
            Align::Right => n.x = hi_x,
            Align::Top => n.y = lo_y,
            Align::VerticalCenters => n.y = round3((lo_y + hi_y) / 2.0),
            Align::Bottom => n.y = hi_y,
        }
    }
}

/// Spreads `nodes` evenly between the two outermost along x (`horizontal`) or y.
pub fn distribute(graph: &mut Graph, nodes: &[usize], horizontal: bool) {
    if nodes.len() < 3 {
        return;
    }
    let at = |g: &Graph, i: usize| {
        let n = &g.nodes[i];
        if horizontal { n.x } else { n.y }
    };
    let mut order = nodes.to_vec();
    order.sort_by(|&a, &b| at(graph, a).total_cmp(&at(graph, b)).then(a.cmp(&b)));
    let (first, last) = (at(graph, order[0]), at(graph, order[order.len() - 1]));
    let steps = (order.len() - 1) as f64;
    for (k, &i) in order.iter().enumerate().skip(1).take(order.len() - 2) {
        let v = round3(first + (last - first) * k as f64 / steps);
        let n = &mut graph.nodes[i];
        if horizontal {
            n.x = v;
        } else {
            n.y = v;
        }
    }
}

/// Moves the centers of `nodes` to the nearest multiples of 0.25.
pub fn snap(graph: &mut Graph, nodes: &[usize]) {
    for &i in nodes {
        let n = &mut graph.nodes[i];
        n.x = (n.x / SNAP).round() * SNAP;
        n.y = (n.y / SNAP).round() * SNAP;
    }
}

/// What a layout works on: the laid-out nodes, numbered from 0 in file order.
struct Part {
    /// Their indices in the graph.
    nodes: Vec<usize>,
    /// Edges between them, as given, without loops and repeats.
    edges: Vec<(usize, usize)>,
    /// A tree's forest: its roots in depth-first order, and each node's children in order.
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
}

impl Part {
    fn new(graph: &Graph, tree: bool, nodes: &[usize]) -> Self {
        let mut nodes = nodes.to_vec();
        nodes.sort_unstable();
        nodes.dedup();
        let local = |i: usize| nodes.binary_search(&i).ok();
        let mut edges = Vec::new();
        for e in &graph.edges {
            if let (Some(a), Some(b)) = (graph.node_index(&e.from), graph.node_index(&e.to))
                && let (Some(a), Some(b)) = (local(a), local(b))
                && a != b
                && !edges.contains(&(a, b))
            {
                edges.push((a, b));
            }
        }
        let (mut roots, mut children) = (Vec::new(), vec![Vec::new(); nodes.len()]);
        if tree {
            // Depth first over the whole tree, then any node it did not reach.
            let mut order = Vec::new();
            let mut stack = vec![0];
            let mut seen = vec![false; graph.nodes.len()];
            while let Some(i) = stack.pop() {
                if i >= graph.nodes.len() || std::mem::replace(&mut seen[i], true) {
                    continue;
                }
                order.push(i);
                stack.extend(atree::children(graph, i).into_iter().rev());
            }
            order.extend((0..graph.nodes.len()).filter(|&i| !seen[i]));
            for i in order {
                let Some(l) = local(i) else { continue };
                match atree::parent(graph, i).and_then(local) {
                    Some(p) => children[p].push(l),
                    None => roots.push(l),
                }
            }
        }
        Self {
            nodes,
            edges,
            roots,
            children,
        }
    }

    fn len(&self) -> usize {
        self.nodes.len()
    }

    /// The forest's nodes depth first.
    fn depth_first(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.len());
        let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(self.children[i].iter().rev());
        }
        order
    }
}

/// Lays `nodes` (every node when empty) of `graph` out by `options`; see
/// docs/features/graph-layout.md#layout.
pub fn layout(graph: &mut Graph, tree: bool, nodes: &[usize], options: &Options) {
    let all: Vec<usize>;
    let nodes = if nodes.is_empty() {
        all = (0..graph.nodes.len()).collect();
        &all
    } else {
        nodes
    };
    let part = Part::new(graph, tree, nodes);
    if part.len() == 0 {
        return;
    }
    let s = options.node_spacing;
    let laid = match (options.algorithm, tree) {
        (Algorithm::Layered, false) => layered(&part, options),
        (Algorithm::Layered, true) => tidy(&part, options),
        (Algorithm::Circle, false) => circle(&part, s),
        (Algorithm::Circle, true) => radial(&part, options),
        (Algorithm::Force, _) => {
            let start: Vec<(f64, f64)> = part
                .nodes
                .iter()
                .map(|&i| (graph.nodes[i].x, graph.nodes[i].y))
                .collect();
            force(&part, start, s)
        }
        (Algorithm::Grid, _) => {
            let order = if tree {
                part.depth_first()
            } else {
                (0..part.len()).collect()
            };
            grid(&order, options.columns, s)
        }
    };
    // Kept where the nodes were: the middle of the bounds, on the 0.25 grid.
    let middle = |points: &mut dyn Iterator<Item = (f64, f64)>| {
        let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for (x, y) in points {
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x), hi.1.max(y));
        }
        ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0)
    };
    let old = middle(
        &mut part
            .nodes
            .iter()
            .map(|&i| (graph.nodes[i].x, graph.nodes[i].y)),
    );
    let target = ((old.0 / SNAP).round() * SNAP, (old.1 / SNAP).round() * SNAP);
    let new = middle(&mut laid.iter().copied());
    for (&i, &(x, y)) in part.nodes.iter().zip(&laid) {
        let n = &mut graph.nodes[i];
        n.x = round3(x - new.0 + target.0);
        n.y = round3(y - new.1 + target.1);
        // No negative zeros in the file.
        n.x += 0.0;
        n.y += 0.0;
    }
}

/// `desired` places in order, each at least `gap` after the one before, as near to them as
/// they can be (least squares): pool adjacent violators on `desired[i] - i × gap`.
fn spaced(desired: &[f64], gap: f64) -> Vec<f64> {
    // Blocks of (sum, count), each one value.
    let mut blocks: Vec<(f64, usize)> = Vec::with_capacity(desired.len());
    for (i, d) in desired.iter().enumerate() {
        blocks.push((d - i as f64 * gap, 1));
        while blocks.len() > 1 {
            let (s1, c1) = blocks[blocks.len() - 1];
            let (s0, c0) = blocks[blocks.len() - 2];
            if s0 / c0 as f64 <= s1 / c1 as f64 {
                break;
            }
            blocks.pop();
            *blocks.last_mut().expect("two blocks") = (s0 + s1, c0 + c1);
        }
    }
    let mut out = Vec::with_capacity(desired.len());
    for (sum, count) in blocks {
        for _ in 0..count {
            let i = out.len();
            out.push(sum / count as f64 + i as f64 * gap);
        }
    }
    out
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

/// Layered (graphs); see docs/features/graph-layout.md#layered-graphs.
fn layered(part: &Part, options: &Options) -> Vec<(f64, f64)> {
    let n = part.len();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in &part.edges {
        out[a].push(b);
    }
    // 1. Cycles: edges back to a node still on the walk are reversed.
    let mut dag: Vec<(usize, usize)> = Vec::new();
    let mut state = vec![0u8; n];
    for start in 0..n {
        if state[start] != 0 {
            continue;
        }
        state[start] = 1;
        let mut stack = vec![(start, 0usize)];
        while let Some(&(u, next)) = stack.last() {
            if let Some(&v) = out[u].get(next) {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
                let edge = if state[v] == 1 { (v, u) } else { (u, v) };
                if !dag.contains(&edge) {
                    dag.push(edge);
                }
                if state[v] == 0 {
                    state[v] = 1;
                    stack.push((v, 0));
                }
            } else {
                state[u] = 2;
                stack.pop();
            }
        }
    }
    let (mut preds, mut succs) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for &(a, b) in &dag {
        succs[a].push(b);
        preds[b].push(a);
    }
    // 2. Layers: the longest path from a node without predecessors.
    let mut layer = vec![0usize; n];
    let mut waiting: Vec<usize> = preds.iter().map(Vec::len).collect();
    let mut ready: std::collections::BTreeSet<usize> =
        (0..n).filter(|&i| waiting[i] == 0).collect();
    while let Some(u) = ready.pop_first() {
        for &v in &succs[u] {
            layer[v] = layer[v].max(layer[u] + 1);
            waiting[v] -= 1;
            if waiting[v] == 0 {
                ready.insert(v);
            }
        }
    }
    let layers = layer.iter().max().map_or(0, |m| m + 1);
    // 3. Order: breadth first from layer 0, then barycenter sweeps.
    let mut rank = vec![usize::MAX; n];
    let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| layer[i] == 0).collect();
    let mut next = 0;
    while let Some(u) = queue.pop_front() {
        if rank[u] != usize::MAX {
            continue;
        }
        rank[u] = next;
        next += 1;
        queue.extend(succs[u].iter().filter(|&&v| rank[v] == usize::MAX));
    }
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    for i in 0..n {
        rows[layer[i]].push(i);
    }
    for row in &mut rows {
        row.sort_by_key(|&i| (rank[i], i));
    }
    // A node's place in its layer, centered on 0.
    let places = |rows: &[Vec<usize>]| {
        let mut place = vec![0.0; n];
        for row in rows {
            for (k, &i) in row.iter().enumerate() {
                place[i] = k as f64 - (row.len() as f64 - 1.0) / 2.0;
            }
        }
        place
    };
    let sweep = |rows: &mut [Vec<usize>], k: usize, by: &[Vec<usize>]| {
        let place = places(rows);
        let mut keyed: Vec<(f64, usize)> = rows[k]
            .iter()
            .map(|&i| (mean(by[i].iter().map(|&j| place[j])).unwrap_or(place[i]), i))
            .collect();
        // Stable: ties keep their order.
        keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
        rows[k] = keyed.into_iter().map(|(_, i)| i).collect();
    };
    for _ in 0..4 {
        for k in 1..layers {
            sweep(&mut rows, k, &preds);
        }
        for k in (0..layers.saturating_sub(1)).rev() {
            sweep(&mut rows, k, &succs);
        }
    }
    // 4. Positions across: centered, then toward the neighbours before, after, before.
    let s = options.node_spacing;
    let mut across = vec![0.0; n];
    for row in &rows {
        for (k, &i) in row.iter().enumerate() {
            across[i] = (k as f64 - (row.len() as f64 - 1.0) / 2.0) * s;
        }
    }
    let pass = |across: &mut Vec<f64>, k: usize, by: &[Vec<usize>]| {
        let desired: Vec<f64> = rows[k]
            .iter()
            .map(|&i| mean(by[i].iter().map(|&j| across[j])).unwrap_or(across[i]))
            .collect();
        for (&i, x) in rows[k].iter().zip(spaced(&desired, s)) {
            across[i] = x;
        }
    };
    for k in 1..layers {
        pass(&mut across, k, &preds);
    }
    for k in (0..layers.saturating_sub(1)).rev() {
        pass(&mut across, k, &succs);
    }
    for k in 1..layers {
        pass(&mut across, k, &preds);
    }
    (0..n)
        .map(|i| {
            let along = layer[i] as f64 * options.layer_spacing;
            options.direction.point(across[i], along)
        })
        .collect()
}

/// Tidy tree; see docs/features/graph-layout.md#tidy-tree-trees.
fn tidy(part: &Part, options: &Options) -> Vec<(f64, f64)> {
    let mut across = vec![0.0; part.len()];
    let mut depth = vec![0usize; part.len()];
    let mut leaves = 0usize;
    fn walk(
        part: &Part,
        i: usize,
        d: usize,
        across: &mut [f64],
        depth: &mut [usize],
        leaves: &mut usize,
        s: f64,
    ) {
        depth[i] = d;
        let kids = &part.children[i];
        if kids.is_empty() {
            across[i] = *leaves as f64 * s;
            *leaves += 1;
            return;
        }
        for &c in kids {
            walk(part, c, d + 1, across, depth, leaves, s);
        }
        across[i] = (across[kids[0]] + across[kids[kids.len() - 1]]) / 2.0;
    }
    for &r in &part.roots {
        walk(
            part,
            r,
            0,
            &mut across,
            &mut depth,
            &mut leaves,
            options.node_spacing,
        );
    }
    (0..part.len())
        .map(|i| {
            let along = depth[i] as f64 * options.layer_spacing;
            options.direction.point(across[i], along)
        })
        .collect()
}

/// The point at `angle` (clockwise from the top) on a circle of `radius`.
fn on_circle(radius: f64, angle: f64) -> (f64, f64) {
    (radius * angle.sin(), -radius * angle.cos())
}

/// Circle (graphs); see docs/features/graph-layout.md#circle-graphs.
fn circle(part: &Part, s: f64) -> Vec<(f64, f64)> {
    let n = part.len();
    let mut near: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in &part.edges {
        for (u, v) in [(a, b), (b, a)] {
            if !near[u].contains(&v) {
                near[u].push(v);
            }
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    for start in 0..n {
        let mut stack = vec![start];
        while let Some(u) = stack.pop() {
            if std::mem::replace(&mut seen[u], true) {
                continue;
            }
            order.push(u);
            stack.extend(near[u].iter().rev().filter(|&&v| !seen[v]));
        }
    }
    let radius = if n > 1 {
        s / (2.0 * (PI / n as f64).sin())
    } else {
        0.0
    };
    let mut out = vec![(0.0, 0.0); n];
    for (k, &i) in order.iter().enumerate() {
        out[i] = on_circle(radius, 2.0 * PI * k as f64 / n as f64);
    }
    out
}

/// Radial (trees); see docs/features/graph-layout.md#radial-trees.
fn radial(part: &Part, options: &Options) -> Vec<(f64, f64)> {
    let n = part.len();
    // Several roots sit on the first ring, around an empty center.
    let first = usize::from(part.roots.len() > 1);
    let mut depth = vec![0usize; n];
    // Each node's first and last leaf, in leaf order.
    let mut span = vec![(0usize, 0usize); n];
    let mut leaves = 0usize;
    fn walk(
        part: &Part,
        i: usize,
        d: usize,
        depth: &mut [usize],
        span: &mut [(usize, usize)],
        leaves: &mut usize,
    ) {
        depth[i] = d;
        let kids = &part.children[i];
        if kids.is_empty() {
            span[i] = (*leaves, *leaves);
            *leaves += 1;
            return;
        }
        for &c in kids {
            walk(part, c, d + 1, depth, span, leaves);
        }
        span[i] = (span[kids[0]].0, span[kids[kids.len() - 1]].1);
    }
    for &r in &part.roots {
        walk(part, r, first, &mut depth, &mut span, &mut leaves);
    }
    let deepest = depth.iter().copied().max().unwrap_or(0);
    if deepest == 0 {
        return vec![(0.0, 0.0); n];
    }
    let step = options
        .layer_spacing
        .max(leaves as f64 * options.node_spacing / (2.0 * PI * deepest as f64));
    (0..n)
        .map(|i| {
            let (a, b) = span[i];
            let angle = PI * (a + b) as f64 / leaves as f64;
            on_circle(depth[i] as f64 * step, angle)
        })
        .collect()
}

/// Force (Fruchterman–Reingold); see docs/features/graph-layout.md#force-graphs-and-trees.
fn force(part: &Part, mut at: Vec<(f64, f64)>, k: f64) -> Vec<(f64, f64)> {
    let n = at.len();
    // Nodes on one spot are spread on a small circle, in file order.
    for i in 0..n {
        let same = (0..i).filter(|&j| at[j] == at[i]).count();
        if same > 0 {
            let (dx, dy) = on_circle(0.1 * k, same as f64);
            at[i] = (at[i].0 + dx, at[i].1 + dy);
        }
    }
    let mut links: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in &part.edges {
        let link = (a.min(b), a.max(b));
        if !links.contains(&link) {
            links.push(link);
        }
    }
    for step in 0..FORCE_STEPS {
        let limit = 2.0 * k * (1.0 - step as f64 / FORCE_STEPS as f64);
        let mut push = vec![(0.0, 0.0); n];
        for i in 0..n {
            for j in i + 1..n {
                let (dx, dy) = (at[i].0 - at[j].0, at[i].1 - at[j].1);
                let d = (dx * dx + dy * dy).sqrt().max(0.01);
                let f = k * k / d / d;
                push[i] = (push[i].0 + dx * f, push[i].1 + dy * f);
                push[j] = (push[j].0 - dx * f, push[j].1 - dy * f);
            }
        }
        for &(i, j) in &links {
            let (dx, dy) = (at[i].0 - at[j].0, at[i].1 - at[j].1);
            let d = (dx * dx + dy * dy).sqrt().max(0.01);
            let f = d / k;
            push[i] = (push[i].0 - dx * f, push[i].1 - dy * f);
            push[j] = (push[j].0 + dx * f, push[j].1 + dy * f);
        }
        let middle = (
            at.iter().map(|p| p.0).sum::<f64>() / n as f64,
            at.iter().map(|p| p.1).sum::<f64>() / n as f64,
        );
        for i in 0..n {
            let (px, py) = (
                push[i].0 - GRAVITY * (at[i].0 - middle.0),
                push[i].1 - GRAVITY * (at[i].1 - middle.1),
            );
            let length = (px * px + py * py).sqrt();
            if length > 0.0 {
                let moved = length.min(limit);
                at[i] = (at[i].0 + px / length * moved, at[i].1 + py / length * moved);
            }
        }
    }
    at
}

/// Grid; see docs/features/graph-layout.md#grid-graphs-and-trees.
fn grid(order: &[usize], columns: u32, s: f64) -> Vec<(f64, f64)> {
    let n = order.len();
    let columns = match columns {
        0 => (n as f64).sqrt().ceil() as usize,
        c => c as usize,
    }
    .max(1);
    let mut out = vec![(0.0, 0.0); n];
    for (k, &i) in order.iter().enumerate() {
        out[i] = ((k % columns) as f64 * s, (k / columns) as f64 * s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agraph::{Edge, Node};

    fn graph(nodes: usize, edges: &[(usize, usize)]) -> Graph {
        let mut g = Graph::new();
        g.nodes = (0..nodes)
            .map(|i| Node::new(format!("n{i}"), i as f64 * 0.37, (i % 3) as f64))
            .collect();
        g.edges = edges
            .iter()
            .map(|&(a, b)| Edge::new(format!("n{a}"), format!("n{b}")))
            .collect();
        g
    }

    fn at(g: &Graph, i: usize) -> (f64, f64) {
        (g.nodes[i].x, g.nodes[i].y)
    }

    fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    }

    #[test]
    fn aligning_and_distributing() {
        let mut g = graph(4, &[]);
        g.nodes[3].x = 5.0;
        align(&mut g, &[0, 1, 2], Align::Right);
        assert_eq!((g.nodes[0].x, g.nodes[2].x), (0.74, 0.74));
        assert_eq!(g.nodes[3].x, 5.0);
        align(&mut g, &[0, 3], Align::HorizontalCenters);
        assert_eq!(g.nodes[0].x, 2.87);
        g.nodes[1].x = 0.0;
        g.nodes[2].x = 9.0;
        distribute(&mut g, &[1, 2, 3, 0], true);
        // 0 at 2.87 and 3 at 5: between 0 and 9 they go to 3 and 6, in x order.
        assert_eq!((g.nodes[0].x, g.nodes[3].x), (3.0, 6.0));
        g.nodes[0].y = 0.13;
        snap(&mut g, &[0]);
        assert_eq!(at(&g, 0), (3.0, 0.25));
    }

    #[test]
    fn layered_graphs_flow_down_and_break_cycles() {
        // A chain with a cycle back to the start, and a side branch.
        let mut g = graph(4, &[(0, 1), (1, 2), (2, 0), (0, 3)]);
        let options = Options::default();
        layout(&mut g, false, &[], &options);
        let y = |i: usize| g.nodes[i].y;
        assert!(y(0) < y(1) && y(1) < y(2), "{:?}", g.nodes);
        assert_eq!(y(1) - y(0), 2.0);
        assert_eq!(y(3), y(1));
        assert!((g.nodes[1].x - g.nodes[3].x).abs() >= 1.5);
        // Again: nothing moves (the result is kept where it was).
        let before = g.clone();
        layout(&mut g, false, &[], &options);
        assert_eq!(g, before);
        // Right: layers along x.
        layout(
            &mut g,
            false,
            &[],
            &Options {
                direction: Direction::Right,
                ..options
            },
        );
        assert!(g.nodes[0].x < g.nodes[1].x && g.nodes[1].x < g.nodes[2].x);
    }

    #[test]
    fn spacing_keeps_order_and_gaps() {
        assert_eq!(spaced(&[0.0, 0.0, 0.0], 1.0), [-1.0, 0.0, 1.0]);
        assert_eq!(spaced(&[0.0, 5.0], 1.0), [0.0, 5.0]);
        assert_eq!(spaced(&[3.0, 0.0], 2.0), [0.5, 2.5]);
    }

    #[test]
    fn tidy_trees_center_parents_over_children() {
        let mut t = atree::new();
        let a = atree::add_child(&mut t, 0, 0.0, 0.0);
        let b = atree::add_child(&mut t, 0, 0.0, 0.0);
        let a1 = atree::add_child(&mut t, a, 0.0, 0.0);
        let a2 = atree::add_child(&mut t, a, 0.0, 0.0);
        let b1 = atree::add_child(&mut t, b, 0.0, 0.0);
        layout(&mut t, true, &[], &Options::default());
        let x = |i: usize| t.nodes[i].x;
        assert_eq!(x(a), (x(a1) + x(a2)) / 2.0);
        assert_eq!(x(0), (x(a) + x(b)) / 2.0);
        assert_eq!((x(a2) - x(a1), x(b1) - x(a2)), (1.5, 1.5));
        assert_eq!(t.nodes[a1].y - t.nodes[0].y, 4.0);
        // Laying out a's subtree only leaves the rest alone.
        let before = t.clone();
        t.nodes[a1].y = 9.0;
        layout(&mut t, true, &[a, a1, a2], &Options::default());
        assert_eq!(at(&t, b1), at(&before, b1));
        assert_eq!(t.nodes[a1].y, t.nodes[a2].y);
    }

    #[test]
    fn circles_radials_grids_and_forces() {
        let s = 1.5;
        let mut g = graph(5, &[(0, 1), (1, 2), (3, 4)]);
        let options = Options {
            algorithm: Algorithm::Circle,
            ..Options::default()
        };
        layout(&mut g, false, &[], &options);
        // Neighbours on the circle are the node spacing apart; 0, 1, 2 are neighbours.
        assert!((distance(at(&g, 0), at(&g, 1)) - s).abs() < 0.01);
        assert!((distance(at(&g, 1), at(&g, 2)) - s).abs() < 0.01);

        let mut t = atree::new();
        let kids: Vec<usize> = (0..4)
            .map(|_| atree::add_child(&mut t, 0, 0.0, 0.0))
            .collect();
        layout(
            &mut t,
            true,
            &[],
            &Options {
                algorithm: Algorithm::Circle,
                ..Options::default()
            },
        );
        for &k in &kids {
            assert!((distance(at(&t, 0), at(&t, k)) - 2.0).abs() < 0.01);
        }

        let mut g = graph(5, &[]);
        layout(
            &mut g,
            false,
            &[],
            &Options {
                algorithm: Algorithm::Grid,
                ..Options::default()
            },
        );
        // 3 columns: 3 then 2.
        assert_eq!(g.nodes[1].x - g.nodes[0].x, 1.5);
        assert_eq!(g.nodes[3].y - g.nodes[0].y, 1.5);
        assert_eq!(g.nodes[3].x, g.nodes[0].x);

        // Force: linked nodes end nearer than unlinked ones; the same every time.
        let mut g = graph(4, &[(0, 1), (2, 3)]);
        let force = Options {
            algorithm: Algorithm::Force,
            ..Options::default()
        };
        let mut again = g.clone();
        layout(&mut g, false, &[], &force);
        layout(&mut again, false, &[], &force);
        assert_eq!(g, again);
        assert!(distance(at(&g, 0), at(&g, 1)) < distance(at(&g, 0), at(&g, 2)));
    }
}
