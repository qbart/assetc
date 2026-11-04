//! Every directed graph and tree: nodes at their positions, edges as arrows.

use assetc_sdk as sdk;
use bevy::prelude::*;

use super::text_2d;
use crate::{ACCENT, BAR, InScene, Spawn, TEXT, TEXT_WEAK};

const NODE: Vec2 = Vec2::new(120.0, 40.0);

/// A graph or tree to lay out: path, nodes (grid position, label, color), edges (node
/// indices) and whether edges are arrows.
type Shown = (
    String,
    Vec<(Vec2, String, [u8; 4])>,
    Vec<(usize, usize)>,
    bool,
);

/// A graph's boxes and arrows in world space, drawn every frame.
#[derive(Component)]
pub struct Drawing {
    /// Center, size and color.
    boxes: Vec<(Vec2, Vec2, Color)>,
    arrows: Vec<(Vec2, Vec2)>,
}

pub fn spawn(s: &mut Spawn) {
    let Some(assets) = s.demo.assets.clone() else {
        return;
    };
    let mut shown: Vec<Shown> = Vec::new();
    for entry in assets.of_kind(sdk::Kind::Graph) {
        if let Ok(g) = assets.load::<sdk::Graph>(entry.id) {
            let nodes: Vec<_> = g
                .nodes
                .iter()
                .map(|n| (Vec2::new(n.x, -n.y), label(&n.id, &n.label), n.color))
                .collect();
            let edges = g
                .edges
                .iter()
                .map(|e| (e.from as usize, e.to as usize))
                .collect();
            shown.push((entry.path.clone(), nodes, edges, true));
        }
    }
    for entry in assets.of_kind(sdk::Kind::Tree) {
        if let Ok(t) = assets.load::<sdk::Tree>(entry.id) {
            let nodes: Vec<_> = t
                .nodes
                .iter()
                .map(|n| (Vec2::new(n.x, -n.y), label(&n.id, &n.label), n.color))
                .collect();
            let edges = t
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| Some((n.parent? as usize, i)))
                .collect();
            shown.push((entry.path.clone(), nodes, edges, false));
        }
    }
    let (width, height) = (s.window.width(), s.window.height());
    let columns = shown.len().clamp(1, 2);
    let rows = shown.len().div_ceil(columns).max(1);
    let cell = Vec2::new(width / columns as f32, (height - BAR * 2.0) / rows as f32);
    for (i, (path, nodes, edges, arrows)) in shown.into_iter().enumerate() {
        let corner = Vec2::new(-width / 2.0, height / 2.0 - BAR)
            + Vec2::new(
                (i % columns) as f32 * cell.x,
                -((i / columns) as f32) * cell.y,
            );
        text_2d(
            &mut s.commands,
            corner + Vec2::new(cell.x / 2.0, -8.0),
            path,
            14.0,
            TEXT,
        );
        let area = Rect::from_corners(
            corner + Vec2::new(24.0, -40.0),
            corner + Vec2::new(cell.x - 24.0, -cell.y + 16.0),
        );
        // Grid units to the area: fit the node centers, keeping the aspect.
        let positions: Vec<Vec2> = nodes.iter().map(|(p, ..)| *p).collect();
        let min = positions
            .iter()
            .copied()
            .reduce(Vec2::min)
            .unwrap_or_default();
        let max = positions
            .iter()
            .copied()
            .reduce(Vec2::max)
            .unwrap_or_default();
        let span = (max - min).max(Vec2::splat(1e-3));
        let room = area.size() - NODE;
        let scale = (room / span).min_element().min(160.0);
        let place = |p: Vec2| area.center() + (p - (min + max) / 2.0) * scale;
        let mut drawing = Drawing {
            boxes: Vec::new(),
            arrows: Vec::new(),
        };
        // A box is as wide as its row leaves room for: up to NODE.x.
        let sizes: Vec<Vec2> = positions
            .iter()
            .map(|p| {
                let room = positions
                    .iter()
                    .filter(|q| (q.y - p.y).abs() < 1e-3 && *q != p)
                    .map(|q| (q.x - p.x).abs() * scale - 8.0)
                    .fold(NODE.x, f32::min);
                Vec2::new(room.max(24.0), NODE.y)
            })
            .collect();
        for ((p, text, [r, g, b, a]), size) in nodes.iter().zip(&sizes) {
            let color = if *a == 0 {
                ACCENT
            } else {
                Color::srgba_u8(*r, *g, *b, 255)
            };
            drawing.boxes.push((place(*p), *size, color));
            s.commands.spawn((
                InScene,
                Text2d::new(text.clone()),
                crate::font(if size.x < NODE.x { 10.0 } else { 12.0 }),
                TextColor(TEXT),
                TextLayout::new(Justify::Center, LineBreak::WordBoundary),
                bevy::text::TextBounds::new_horizontal(size.x - 6.0),
                Transform::from_translation(place(*p).extend(1.0)),
            ));
        }
        for (from, to) in edges {
            let (Some(a), Some(b)) = (positions.get(from), positions.get(to)) else {
                continue;
            };
            let (a, b) = (place(*a), place(*b));
            if from == to {
                continue;
            }
            // From box edge to box edge.
            let dir = (b - a).normalize_or_zero();
            let inset = |size: Vec2| (size / 2.0 / dir.abs().max(Vec2::splat(1e-3))).min_element();
            let (start, end) = (a + dir * inset(sizes[from]), b - dir * inset(sizes[to]));
            drawing.arrows.push((start, end));
        }
        s.commands.spawn((InScene, drawing, TreeLines(!arrows)));
    }
}

/// Whether a drawing's edges are tree lines (no arrow heads).
#[derive(Component)]
pub struct TreeLines(bool);

/// A node's label, else its id; with characters the default font lacks spelled out.
fn label(id: &str, label: &str) -> String {
    let text = if label.is_empty() { id } else { label };
    text.replace('…', "...")
}

pub fn draw(mut gizmos: Gizmos, drawings: Query<(&Drawing, &TreeLines)>) {
    for (drawing, tree) in &drawings {
        for (center, size, color) in &drawing.boxes {
            gizmos.rect_2d(Isometry2d::from_translation(*center), *size, *color);
        }
        for (a, b) in &drawing.arrows {
            if tree.0 {
                gizmos.line_2d(*a, *b, TEXT_WEAK);
            } else {
                gizmos.arrow_2d(*a, *b, TEXT_WEAK).with_tip_length(10.0);
            }
        }
    }
}
