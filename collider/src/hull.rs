//! 3D convex hulls (quickhull).

use crate::math::{AsDouble, DVec3, Ext, Vec3};

/// A convex hull: its corner points and its triangles, as indices into `points`, wound
/// counter-clockwise seen from outside.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hull {
    pub points: Vec<[f32; 3]>,
    pub faces: Vec<[usize; 3]>,
}

impl Hull {
    /// Each edge once, as indices into `points`, sorted.
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

    /// The volume it encloses.
    pub fn volume(&self) -> f32 {
        Poly::from(self).volume()
    }
}

/// The convex hull of `points`; `None` when they do not span a volume (fewer than four
/// points, all on one plane, or not all finite).
pub fn hull(points: &[[f32; 3]]) -> Option<Hull> {
    let points: Vec<Vec3> = points.iter().map(|&p| Vec3::from(p)).collect();
    quickhull(&points).map(|p| Hull::from(&p))
}

/// A hull in the crate's own vectors, as the decomposition works on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Poly {
    pub points: Vec<Vec3>,
    pub faces: Vec<[usize; 3]>,
}

impl Poly {
    /// Volume enclosed.
    pub fn volume(&self) -> f32 {
        let Some(&o) = self.points.first() else {
            return 0.0;
        };
        self.faces
            .iter()
            .map(|&[a, b, c]| {
                let [a, b, c] = [a, b, c].map(|i| self.points[i] - o);
                a.dot(b.cross(c))
            })
            .sum::<f32>()
            / 6.0
    }
}

impl From<&Poly> for Hull {
    fn from(p: &Poly) -> Self {
        Self {
            points: p.points.iter().map(|v| v.to_array()).collect(),
            faces: p.faces.clone(),
        }
    }
}

impl From<&Hull> for Poly {
    fn from(h: &Hull) -> Self {
        Self {
            points: h.points.iter().map(|&p| Vec3::from(p)).collect(),
            faces: h.faces.clone(),
        }
    }
}

struct Face {
    v: [usize; 3],
    /// The face across each edge `v[k]`–`v[k + 1]`.
    next: [usize; 3],
    normal: DVec3,
    offset: f64,
    /// Points above the face, not yet on the hull.
    outside: Vec<usize>,
    alive: bool,
}

impl Face {
    fn new(points: &[DVec3], v: [usize; 3]) -> Self {
        let [a, b, c] = v.map(|i| points[i]);
        let normal = (b - a).cross(c - a).normalize_or_zero();
        Self {
            v,
            next: [usize::MAX; 3],
            normal,
            offset: normal.dot(a),
            outside: Vec::new(),
            alive: true,
        }
    }

    fn distance(&self, p: DVec3) -> f64 {
        self.normal.dot(p) - self.offset
    }

    /// Which edge of the face goes from `a` to `b`.
    fn edge(&self, a: usize, b: usize) -> Option<usize> {
        (0..3).find(|&k| self.v[k] == a && self.v[(k + 1) % 3] == b)
    }
}

/// The convex hull of `points`; `None` when they do not span a volume.
pub(crate) fn quickhull(points: &[Vec3]) -> Option<Poly> {
    if !points.iter().all(|p| p.is_finite()) {
        return None;
    }
    let input = points;
    let points: Vec<DVec3> = points.iter().map(|p| p.as_dvec3()).collect();
    let (mut min, mut max) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for &p in &points {
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
    let base = Face::new(&points, [a, b, c]);
    let d = (0..points.len()).max_by(|&i, &j| {
        base.distance(points[i])
            .abs()
            .total_cmp(&base.distance(points[j]).abs())
    })?;
    if base.distance(points[d]).abs() <= eps {
        return None;
    }
    // `d` above `a b c`: wind the base the other way so every face faces out.
    let (b, c) = if base.distance(points[d]) > 0.0 {
        (c, b)
    } else {
        (b, c)
    };
    let mut faces: Vec<Face> = [[a, b, c], [a, d, b], [b, d, c], [c, d, a]]
        .into_iter()
        .map(|v| Face::new(&points, v))
        .collect();
    link(&mut faces, &[0, 1, 2, 3]);
    assign(
        &points,
        &mut faces,
        &[0, 1, 2, 3],
        (0..points.len()).filter(|i| ![a, b, c, d].contains(i)),
        eps,
    );

    // Grow the hull to the farthest outside point of a face, until none is left.
    let mut pending: Vec<usize> = (0..4).collect();
    let mut visible_mark = vec![false; 4];
    while let Some(f) = pending.pop() {
        if !faces[f].alive || faces[f].outside.is_empty() {
            continue;
        }
        let face = &faces[f];
        let p = *face
            .outside
            .iter()
            .max_by(|&&i, &&j| {
                face.distance(points[i])
                    .total_cmp(&face.distance(points[j]))
            })
            .expect("an outside point");
        let eye = points[p];
        // The faces `p` sees, from `f` across neighbours.
        let mut visible = vec![f];
        visible_mark.resize(faces.len(), false);
        visible_mark[f] = true;
        let mut next = 0;
        while next < visible.len() {
            let g = visible[next];
            next += 1;
            for k in 0..3 {
                let h = faces[g].next[k];
                if !visible_mark[h] && faces[h].distance(eye) > eps {
                    visible_mark[h] = true;
                    visible.push(h);
                }
            }
        }
        // Edges of visible faces whose other side is not visible: the horizon, each with the
        // face beyond it.
        let mut horizon = Vec::new();
        for &g in &visible {
            for k in 0..3 {
                let h = faces[g].next[k];
                if !visible_mark[h] {
                    horizon.push(([faces[g].v[k], faces[g].v[(k + 1) % 3]], h));
                }
            }
        }
        // Rounding can leave a horizon that is not one loop: then `p` is taken as on the hull
        // already and dropped.
        let starts: Vec<usize> = horizon.iter().map(|(e, _)| e[0]).collect();
        let loop_ok = horizon.iter().all(|&([x, y], beyond)| {
            starts.iter().filter(|&&s| s == y).count() == 1
                && starts.iter().filter(|&&s| s == x).count() == 1
                && beyond != usize::MAX
                && faces[beyond].edge(y, x).is_some()
        });
        if !loop_ok {
            for &g in &visible {
                visible_mark[g] = false;
            }
            faces[f].outside.retain(|&i| i != p);
            pending.push(f);
            continue;
        }
        let mut orphans = Vec::new();
        for &g in &visible {
            faces[g].alive = false;
            visible_mark[g] = false;
            orphans.append(&mut faces[g].outside);
        }
        let first = faces.len();
        for &([x, y], beyond) in &horizon {
            let new = faces.len();
            let mut face = Face::new(&points, [x, y, p]);
            face.next[0] = beyond;
            if let Some(k) = faces[beyond].edge(y, x) {
                faces[beyond].next[k] = new;
            }
            faces.push(face);
        }
        let created: Vec<usize> = (first..faces.len()).collect();
        // New faces meet each other along their edges to `p`.
        for &i in &created {
            let y = faces[i].v[1];
            if let Some(&j) = created.iter().find(|&&j| faces[j].v[0] == y) {
                faces[i].next[1] = j;
                faces[j].next[2] = i;
            }
        }
        orphans.retain(|&i| i != p);
        assign(&points, &mut faces, &created, orphans.into_iter(), eps);
        pending.extend(
            created
                .iter()
                .copied()
                .filter(|&i| !faces[i].outside.is_empty()),
        );
    }

    // Keep the points the faces use, renumbered.
    let mut index = vec![usize::MAX; points.len()];
    let mut out = Poly::default();
    for face in faces.iter().filter(|f| f.alive) {
        out.faces.push(face.v.map(|i| {
            if index[i] == usize::MAX {
                index[i] = out.points.len();
                out.points.push(input[i]);
            }
            index[i]
        }));
    }
    Some(out)
}

/// Links the faces `ids` to each other across the edges they share.
fn link(faces: &mut [Face], ids: &[usize]) {
    for &i in ids {
        for k in 0..3 {
            let (a, b) = (faces[i].v[k], faces[i].v[(k + 1) % 3]);
            if let Some(&j) = ids.iter().find(|&&j| faces[j].edge(b, a).is_some()) {
                faces[i].next[k] = j;
            }
        }
    }
}

/// Puts each of `candidates` in the outside set of the face of `ids` it is farthest above.
fn assign(
    points: &[DVec3],
    faces: &mut [Face],
    ids: &[usize],
    candidates: impl Iterator<Item = usize>,
    eps: f64,
) {
    for i in candidates {
        let best = ids
            .iter()
            .map(|&f| (f, faces[f].distance(points[i])))
            .filter(|&(_, d)| d > eps)
            .max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((f, _)) = best {
            faces[f].outside.push(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hull(points: &[Vec3]) -> Option<Poly> {
        quickhull(points)
    }

    fn cube_corners() -> Vec<Vec3> {
        (0..8)
            .map(|i| Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, ((i >> 2) & 1) as f32))
            .collect()
    }

    /// Every point is inside or on the hull, every face faces out.
    fn check(points: &[Vec3], hull: &Poly) {
        let center = hull.points.iter().sum::<Vec3>() / hull.points.len() as f32;
        for &[a, b, c] in &hull.faces {
            let [a, b, c] = [a, b, c].map(|i| hull.points[i]);
            let normal = (b - a).cross(c - a).normalize();
            assert!(normal.dot(a - center) > 0.0, "faces out");
            for &p in points {
                assert!(normal.dot(p - a) <= 1e-4, "{p:?} outside");
            }
        }
        // Closed: every edge in exactly two faces, a solid's Euler characteristic.
        let (v, e, f) = (
            hull.points.len(),
            Hull::from(hull).edges().len(),
            hull.faces.len(),
        );
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
        assert!(hull(&[Vec3::splat(f32::NAN), Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z]).is_none());
        // The public form gives the same hull.
        let corners: Vec<[f32; 3]> = cube_corners().iter().map(|p| p.to_array()).collect();
        assert_eq!(
            super::hull(&corners).unwrap(),
            Hull::from(&quickhull(&cube_corners()).unwrap())
        );
        assert!((super::hull(&corners).unwrap().volume() - 1.0).abs() < 1e-6);
    }
}
