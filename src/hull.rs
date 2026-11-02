//! 3D convex hulls (quickhull) for convex hull shapes; see docs/features/shapes.md#shape.

use bevy::math::Vec3;

/// A convex hull: its corner points and its triangles, as indices into `points`, wound
/// counter-clockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hull {
    pub points: Vec<Vec3>,
    pub faces: Vec<[usize; 3]>,
}

impl Hull {
    /// Each edge once, as indices into `points`.
    pub fn edges(&self) -> Vec<[usize; 2]> {
        let mut edges: Vec<[usize; 2]> = self
            .faces
            .iter()
            .flat_map(|&[a, b, c]| [[a, b], [b, c], [c, a]])
            .map(|[a, b]| [a.min(b), a.max(b)])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        edges
    }
}

struct Face {
    v: [usize; 3],
    normal: Vec3,
    offset: f32,
    /// Points above the face, not yet on the hull.
    outside: Vec<usize>,
    alive: bool,
}

impl Face {
    fn new(points: &[Vec3], v: [usize; 3]) -> Self {
        let [a, b, c] = v.map(|i| points[i]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        Self {
            v,
            normal,
            offset: normal.dot(a),
            outside: Vec::new(),
            alive: true,
        }
    }

    fn distance(&self, p: Vec3) -> f32 {
        self.normal.dot(p) - self.offset
    }
}

/// The convex hull of `points`; `None` when they do not span a volume (fewer than four
/// points, or all on one plane).
pub fn hull(points: &[Vec3]) -> Option<Hull> {
    let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for &p in points {
        (min, max) = (min.min(p), max.max(p));
    }
    let size = (max - min).max_element();
    // Also stops NaN sizes.
    if size.is_nan() || size <= 0.0 {
        return None;
    }
    let eps = size * 1e-5;

    // A first tetrahedron: two extreme points, the farthest from their line, the farthest
    // from that plane.
    let axis = (max - min).max_position();
    let a = (0..points.len()).min_by(|&i, &j| points[i][axis].total_cmp(&points[j][axis]))?;
    let b = (0..points.len()).max_by(|&i, &j| points[i][axis].total_cmp(&points[j][axis]))?;
    let line = (points[b] - points[a]).normalize_or_zero();
    let off_line = |i: usize| {
        let d = points[i] - points[a];
        (d - line * d.dot(line)).length()
    };
    let c = (0..points.len()).max_by(|&i, &j| off_line(i).total_cmp(&off_line(j)))?;
    if off_line(c) <= eps {
        return None;
    }
    let base = Face::new(points, [a, b, c]);
    let d = (0..points.len()).max_by(|&i, &j| {
        base.distance(points[i])
            .abs()
            .total_cmp(&base.distance(points[j]).abs())
    })?;
    if base.distance(points[d]).abs() <= eps {
        return None;
    }
    let mut faces: Vec<Face> = if base.distance(points[d]) > 0.0 {
        // `d` is above `a b c`: wind the base the other way so every face faces out.
        vec![
            Face::new(points, [a, c, b]),
            Face::new(points, [a, b, d]),
            Face::new(points, [b, c, d]),
            Face::new(points, [c, a, d]),
        ]
    } else {
        vec![
            Face::new(points, [a, b, c]),
            Face::new(points, [a, d, b]),
            Face::new(points, [b, d, c]),
            Face::new(points, [c, d, a]),
        ]
    };
    assign(
        points,
        &mut faces,
        (0..points.len()).filter(|i| ![a, b, c, d].contains(i)),
        eps,
    );

    // Grow the hull to the farthest outside point of a face, until none is left.
    while let Some(f) = faces.iter().position(|f| f.alive && !f.outside.is_empty()) {
        let face = &faces[f];
        let p = *face
            .outside
            .iter()
            .max_by(|&&i, &&j| {
                face.distance(points[i])
                    .total_cmp(&face.distance(points[j]))
            })
            .expect("an outside point");
        let visible: Vec<usize> = (0..faces.len())
            .filter(|&i| faces[i].alive && faces[i].distance(points[p]) > eps)
            .collect();
        // Edges of visible faces whose other side is not visible: the horizon.
        let edges: Vec<[usize; 2]> = visible
            .iter()
            .flat_map(|&i| {
                let [x, y, z] = faces[i].v;
                [[x, y], [y, z], [z, x]]
            })
            .collect();
        let horizon: Vec<[usize; 2]> = edges
            .iter()
            .filter(|[x, y]| !edges.contains(&[*y, *x]))
            .copied()
            .collect();
        let mut orphans = Vec::new();
        for &i in &visible {
            faces[i].alive = false;
            orphans.append(&mut faces[i].outside);
        }
        let first = faces.len();
        for [x, y] in horizon {
            faces.push(Face::new(points, [x, y, p]));
        }
        orphans.retain(|&i| i != p);
        assign(points, &mut faces[first..], orphans.into_iter(), eps);
    }

    // Keep the points the faces use, renumbered.
    let mut index = vec![usize::MAX; points.len()];
    let mut out = Hull::default();
    for face in faces.iter().filter(|f| f.alive) {
        out.faces.push(face.v.map(|i| {
            if index[i] == usize::MAX {
                index[i] = out.points.len();
                out.points.push(points[i]);
            }
            index[i]
        }));
    }
    Some(out)
}

/// Puts each of `candidates` in the outside set of the first face it is above.
fn assign(points: &[Vec3], faces: &mut [Face], candidates: impl Iterator<Item = usize>, eps: f32) {
    for i in candidates {
        if let Some(face) = faces
            .iter_mut()
            .find(|f| f.alive && f.distance(points[i]) > eps)
        {
            face.outside.push(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_corners() -> Vec<Vec3> {
        (0..8)
            .map(|i| Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32))
            .collect()
    }

    /// Every point is inside or on the hull, every face faces out.
    fn check(points: &[Vec3], hull: &Hull) {
        let center = hull.points.iter().sum::<Vec3>() / hull.points.len() as f32;
        for &[a, b, c] in &hull.faces {
            let [a, b, c] = [a, b, c].map(|i| hull.points[i]);
            let normal = (b - a).cross(c - a).normalize();
            assert!(normal.dot(a - center) > 0.0, "faces out");
            for &p in points {
                assert!(normal.dot(p - a) <= 1e-4, "{p} outside");
            }
        }
        // Closed: every edge in exactly two faces, a solid's Euler characteristic.
        let (v, e, f) = (hull.points.len(), hull.edges().len(), hull.faces.len());
        assert_eq!(v as i64 - e as i64 + f as i64, 2);
    }

    #[test]
    fn cube_with_inside_points() {
        let mut points = cube_corners();
        points.extend([
            Vec3::splat(0.5),
            Vec3::new(0.2, 0.7, 0.4),
            Vec3::new(0.5, 0.5, 1.0),
        ]);
        let hull = hull(&points).unwrap();
        // The corners only (the face center lies on a face).
        assert_eq!(hull.points.len(), 8, "{:?}", hull.points);
        check(&points, &hull);
    }

    #[test]
    fn sphere_points() {
        let mut points = Vec::new();
        for i in 0..20 {
            for j in 0..20 {
                let (t, p) = (i as f32 * 0.31, j as f32 * 0.17);
                points.push(Vec3::new(t.cos() * p.sin(), p.cos(), t.sin() * p.sin()) * 2.0);
            }
        }
        let hull = hull(&points).unwrap();
        check(&points, &hull);
    }

    #[test]
    fn flat_or_too_few_points_make_none() {
        assert!(hull(&[Vec3::ZERO, Vec3::X, Vec3::Y]).is_none());
        let flat: Vec<Vec3> = (0..10)
            .map(|i| Vec3::new(i as f32, (i * i) as f32, 0.0))
            .collect();
        assert!(hull(&flat).is_none());
        assert!(hull(&[]).is_none());
    }
}
