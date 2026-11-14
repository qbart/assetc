//! Distance maps of texture paints: how far every spot of a model is from a point on it,
//! along the surface or in a straight line; see docs/features/texture-paint.md#distance-map.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use bevy::math::{Vec2, Vec3};

/// How a distance is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Measure {
    /// The shortest way over the mesh.
    #[default]
    Surface,
    /// The direct distance through space.
    Straight,
}

impl Measure {
    pub const ALL: [Self; 2] = [Self::Surface, Self::Straight];

    pub fn key(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Straight => "straight",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Surface => "Along the surface",
            Self::Straight => "Straight line",
        }
    }
}

/// A point on a model's surface: a glTF triangle and the place in it, so it stays on the
/// surface however the model is posed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub mesh: u32,
    pub primitive: u32,
    /// The triangle's index in the primitive.
    pub triangle: u32,
    /// The weights of the triangle's second and third corner (the first's is the rest).
    pub at: [f32; 2],
}

/// One primitive of a model as posed.
pub struct Part<'a> {
    /// Its glTF mesh and primitive.
    pub id: Option<(u32, u32)>,
    pub positions: &'a [Vec3],
    /// Triangle lists, as indices into `positions`.
    pub indices: &'a [usize],
    /// `TEXCOORD_0` per vertex, when it has it.
    pub uvs: Option<&'a [[f32; 2]]>,
}

impl Part<'_> {
    /// The corners of triangle `t`, `None` when it has none or an index is out of range.
    fn triangle(&self, t: usize) -> Option<([usize; 3], [Vec3; 3])> {
        let corners: [usize; 3] = self.indices.get(t * 3..t * 3 + 3)?.try_into().ok()?;
        let [a, b, c] = corners.map(|i| self.positions.get(i).copied());
        Some((corners, [a?, b?, c?]))
    }

    fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Where `point` is on `parts`: the part, the triangle in it and the position.
pub fn locate(parts: &[Part], point: &Point) -> Option<(usize, usize, Vec3)> {
    let part = parts
        .iter()
        .position(|p| p.id == Some((point.mesh, point.primitive)))?;
    let (_, [a, b, c]) = parts[part].triangle(point.triangle as usize)?;
    let [k1, k2] = point.at;
    Some((
        part,
        point.triangle as usize,
        a * (1.0 - k1 - k2) + b * k1 + c * k2,
    ))
}

/// The distances of a model's surface from a point on it.
pub struct Field {
    origin: Vec3,
    /// The part and triangle the point lies in.
    home: (usize, usize),
    /// Along the surface: the mesh and how far each of its nodes is.
    surface: Option<(Welded, Vec<f32>)>,
    /// The largest distance of a vertex.
    pub farthest: f32,
}

/// Measures `parts` from `origin`, a point in triangle `home.1` of part `home.0`.
pub fn measure(parts: &[Part], origin: Vec3, home: (usize, usize), measure: Measure) -> Field {
    let (surface, farthest) = match measure {
        Measure::Straight => {
            let farthest = parts
                .iter()
                .flat_map(|part| part.indices.iter().filter_map(|&i| part.positions.get(i)))
                .map(|p| p.distance(origin))
                .filter(|d| d.is_finite())
                .fold(0.0, f32::max);
            (None, farthest)
        }
        Measure::Surface => {
            let mesh = Welded::new(parts);
            // From the point straight to everything on its triangle's edges.
            let seeds: Vec<(u32, f32)> = mesh
                .face(home.0, home.1)
                .iter()
                .filter(|&&node| node != UNUSED)
                .map(|&node| (node, mesh.positions[node as usize].distance(origin)))
                .collect();
            let distances = mesh.distances(&seeds);
            let farthest = distances[..mesh.vertices]
                .iter()
                .copied()
                .filter(|d| d.is_finite())
                .fold(0.0, f32::max);
            (Some((mesh, distances)), farthest)
        }
    };
    Field {
        origin,
        home,
        surface,
        farthest,
    }
}

const UNUSED: u32 = u32::MAX;

/// The surface of a model as a graph to find shortest ways on. Its nodes are the vertices of
/// the triangles, those at one position made one, and points along the triangles' edges; a
/// way goes straight across a triangle from any node on its edges to any other, and over
/// the bridges between parts that do not touch.
struct Welded {
    /// How many nodes are vertices: the first ones.
    vertices: usize,
    positions: Vec<Vec3>,
    /// Per node: the nodes it reaches straight and how far they are.
    edges: Vec<Vec<(u32, f32)>>,
    /// Per part, per triangle: the `stride` nodes on its edges, its corners first;
    /// [`UNUSED`] for a triangle that is left out.
    faces: Vec<Vec<u32>>,
    stride: usize,
    /// The bridges: two vertices of parts that do not touch and the gap between them.
    #[cfg(test)]
    gaps: Vec<(u32, u32, f32)>,
}

impl Welded {
    fn new(parts: &[Part]) -> Self {
        // The vertices, and the triangles between them.
        let mut positions: Vec<Vec3> = Vec::new();
        let mut by_position: HashMap<[u32; 3], u32> = HashMap::new();
        let mut triangles: Vec<Vec<Option<[u32; 3]>>> = Vec::new();
        for part in parts {
            let welded = (0..part.triangles()).map(|t| {
                let (_, corners) = part.triangle(t)?;
                if corners.iter().any(|p| !p.is_finite()) {
                    return None;
                }
                Some(corners.map(|p| {
                    // -0.0 is 0.0.
                    let key = p.to_array().map(|c| (c + 0.0).to_bits());
                    *by_position.entry(key).or_insert_with(|| {
                        positions.push(p);
                        positions.len() as u32 - 1
                    })
                }))
            });
            triangles.push(welded.collect());
        }
        let vertices = positions.len();
        let mut touching = Sets::new(vertices);
        for [a, b, c] in triangles.iter().flatten().flatten() {
            touching.join(*a, *b);
            touching.join(*b, *c);
        }
        let gaps = bridges(&positions, touching);
        // Points along the edges: more of them on a coarse mesh, where a triangle is a
        // large part of the way.
        let count = triangles.iter().flatten().flatten().count();
        let along: usize = match count {
            0..=20_000 => 3,
            20_001..=60_000 => 2,
            _ => 1,
        };
        let stride = 3 + 3 * along;
        let mut edges: Vec<Vec<(u32, f32)>> = vec![Vec::new(); vertices];
        let mut on_edge: HashMap<(u32, u32), u32> = HashMap::new();
        let mut faces = Vec::new();
        for part in &triangles {
            let mut nodes = Vec::with_capacity(part.len() * stride);
            for triangle in part {
                let Some(corners) = triangle else {
                    nodes.extend(std::iter::repeat_n(UNUSED, stride));
                    continue;
                };
                let first = nodes.len();
                nodes.extend(corners);
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    let (a, b) = (corners[a].min(corners[b]), corners[a].max(corners[b]));
                    let start = *on_edge.entry((a, b)).or_insert_with(|| {
                        let (from, to) = (positions[a as usize], positions[b as usize]);
                        for k in 1..=along {
                            positions.push(from.lerp(to, k as f32 / (along + 1) as f32));
                            edges.push(Vec::new());
                        }
                        (positions.len() - along) as u32
                    });
                    nodes.extend(start..start + along as u32);
                }
                let face = &nodes[first..];
                for (i, &a) in face.iter().enumerate() {
                    for &b in &face[i + 1..] {
                        if a == b {
                            continue;
                        }
                        let length = positions[a as usize].distance(positions[b as usize]);
                        edges[a as usize].push((b, length));
                        edges[b as usize].push((a, length));
                    }
                }
            }
            faces.push(nodes);
        }
        for &(a, b, gap) in &gaps {
            edges[a as usize].push((b, gap));
            edges[b as usize].push((a, gap));
        }
        Self {
            vertices,
            positions,
            edges,
            faces,
            stride,
            #[cfg(test)]
            gaps,
        }
    }

    /// The nodes on the edges of triangle `t` of part `part`; none when there is no such
    /// triangle.
    fn face(&self, part: usize, t: usize) -> &[u32] {
        self.faces
            .get(part)
            .and_then(|nodes| nodes.get(t * self.stride..(t + 1) * self.stride))
            .unwrap_or(&[])
    }

    /// The length of the shortest way to each node from `seeds`, nodes and how far they
    /// already are (Dijkstra).
    fn distances(&self, seeds: &[(u32, f32)]) -> Vec<f32> {
        let mut out = vec![f32::INFINITY; self.positions.len()];
        // Distances are not negative, so their bits order as they do.
        let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
        for &(v, d) in seeds {
            if d < out[v as usize] {
                out[v as usize] = d;
                heap.push(Reverse((d.to_bits(), v)));
            }
        }
        while let Some(Reverse((bits, v))) = heap.pop() {
            let d = f32::from_bits(bits);
            if d > out[v as usize] {
                continue;
            }
            for &(w, length) in &self.edges[v as usize] {
                let next = d + length;
                if next < out[w as usize] {
                    out[w as usize] = next;
                    heap.push(Reverse((next.to_bits(), w)));
                }
            }
        }
        out
    }
}

/// The shortest gaps that join the parts of a mesh that do not touch into one: pairs of
/// vertices and how far apart they are; see docs/features/texture-paint.md#measure.
/// `touching` holds the vertices of each part. Each round joins every part to its nearest
/// neighbor (Borůvka), the nearest vertices found through a grid.
fn bridges(positions: &[Vec3], mut parts: Sets) -> Vec<(u32, u32, f32)> {
    let n = positions.len();
    let mut out = Vec::new();
    if n == 0 || parts.count == 1 {
        return out;
    }
    let grid = Grid::new(positions);
    while parts.count > 1 {
        let roots: Vec<u32> = (0..n as u32).map(|v| parts.root(v)).collect();
        let single = grid.single(&roots);
        let mut members: HashMap<u32, Vec<u32>> = HashMap::new();
        for (v, &root) in roots.iter().enumerate() {
            members.entry(root).or_default().push(v as u32);
        }
        // The small parts first: what they join need not look for a neighbor itself.
        let mut order: Vec<(u32, Vec<u32>)> = members.into_iter().collect();
        order.sort_by_key(|(root, vertices)| (vertices.len(), *root));
        let mut joined = vec![false; n];
        let mut found = Vec::new();
        for (root, vertices) in &order {
            if joined[*root as usize] {
                continue;
            }
            let mut best: Option<(f32, u32, u32)> = None;
            for &v in vertices {
                grid.nearest_other(positions, &roots, &single, v, &mut best);
            }
            if let Some((gap, a, b)) = best {
                joined[*root as usize] = true;
                joined[roots[b as usize] as usize] = true;
                found.push((a, b, gap));
            }
        }
        if found.is_empty() {
            break;
        }
        for &(a, b, _) in &found {
            parts.join(a, b);
        }
        out.extend(found);
    }
    out
}

/// Disjoint sets of vertices (union-find).
struct Sets {
    parent: Vec<u32>,
    count: usize,
}

impl Sets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n as u32).collect(),
            count: n,
        }
    }

    fn root(&mut self, mut v: u32) -> u32 {
        while self.parent[v as usize] != v {
            let up = self.parent[v as usize];
            self.parent[v as usize] = self.parent[up as usize];
            v = up;
        }
        v
    }

    fn join(&mut self, a: u32, b: u32) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.parent[a as usize] = b;
            self.count -= 1;
        }
    }
}

/// Vertices sorted into the cells of a grid over their bounds, to find near ones.
struct Grid {
    min: Vec3,
    cell: f32,
    dims: [usize; 3],
    cells: Vec<Vec<u32>>,
}

/// A cell without vertices, and one with vertices of several parts.
const EMPTY: u32 = u32::MAX;
const MIXED: u32 = u32::MAX - 1;

impl Grid {
    fn new(positions: &[Vec3]) -> Self {
        let (mut min, mut max) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        for &p in positions {
            (min, max) = (min.min(p), max.max(p));
        }
        // About four vertices a cell, and few enough cells to walk them all when the
        // nearest neighbor is far.
        let per_axis = ((positions.len() as f32 / 4.0).cbrt().ceil() as usize).clamp(1, 24);
        let cell = ((max - min).max_element() / per_axis as f32).max(1e-12);
        let dims = ((max - min) / cell)
            .to_array()
            .map(|d| (d.floor() as usize + 1).min(per_axis + 1));
        let mut grid = Self {
            min,
            cell,
            dims,
            cells: vec![Vec::new(); dims[0] * dims[1] * dims[2]],
        };
        for (v, &p) in positions.iter().enumerate() {
            let at = grid.cell_of(p);
            let index = grid.index(at);
            grid.cells[index].push(v as u32);
        }
        grid
    }

    fn cell_of(&self, p: Vec3) -> [usize; 3] {
        let at = ((p - self.min) / self.cell).to_array();
        [0, 1, 2].map(|k| (at[k].floor().max(0.0) as usize).min(self.dims[k] - 1))
    }

    fn index(&self, [x, y, z]: [usize; 3]) -> usize {
        x + self.dims[0] * (y + self.dims[1] * z)
    }

    /// Per cell: the one part (by its root) its vertices belong to, [`MIXED`] or [`EMPTY`].
    fn single(&self, roots: &[u32]) -> Vec<u32> {
        self.cells
            .iter()
            .map(|vertices| {
                let mut all = EMPTY;
                for &v in vertices {
                    let root = roots[v as usize];
                    if all == EMPTY {
                        all = root;
                    } else if all != root {
                        return MIXED;
                    }
                }
                all
            })
            .collect()
    }

    /// Improves `best` (gap, vertex, vertex of another part) with the vertex of another part
    /// nearest to `v`, looking no farther than `best` reaches.
    fn nearest_other(
        &self,
        positions: &[Vec3],
        roots: &[u32],
        single: &[u32],
        v: u32,
        best: &mut Option<(f32, u32, u32)>,
    ) {
        let (p, root) = (positions[v as usize], roots[v as usize]);
        let home = self.cell_of(p).map(|c| c as i64);
        let rings = *self.dims.iter().max().unwrap() as i64;
        for ring in 0..=rings {
            // Every cell of this ring is at least this far.
            if let Some((gap, ..)) = *best
                && (ring - 1) as f32 * self.cell >= gap
            {
                break;
            }
            let range =
                |k: usize| (home[k] - ring).max(0)..=(home[k] + ring).min(self.dims[k] as i64 - 1);
            for z in range(2) {
                for y in range(1) {
                    let edge = (z - home[2]).abs() == ring || (y - home[1]).abs() == ring;
                    let mut visit = |x: i64| {
                        let index = self.index([x as usize, y as usize, z as usize]);
                        if single[index] == EMPTY || single[index] == root {
                            return;
                        }
                        for &w in &self.cells[index] {
                            if roots[w as usize] == root {
                                continue;
                            }
                            let gap = p.distance(positions[w as usize]);
                            if best.is_none_or(|(g, ..)| gap < g) {
                                *best = Some((gap, v, w));
                            }
                        }
                    };
                    if edge {
                        range(0).for_each(&mut visit);
                    } else {
                        // Only the two ends of the row are on the ring.
                        for x in [home[0] - ring, home[0] + ring] {
                            if (0..self.dims[0] as i64).contains(&x) {
                                visit(x);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The values of a `size` × `size` channel for `field`: per texel `distance / range` as
/// 0–255, with `range` 0 or less the farthest vertex's distance; 0 where no triangle is. See
/// docs/features/texture-paint.md#distance-map.
pub fn rasterize(parts: &[Part], field: &Field, range: f32, size: u32) -> Vec<u8> {
    let range = if range > 0.0 { range } else { field.farthest };
    let range = if range > 0.0 { range } else { 1.0 };
    let mut raster = Raster {
        size: size as usize,
        values: vec![0.0; size as usize * size as usize],
        cover: vec![0; size as usize * size as usize],
    };
    let s = size as f32;
    for (index, part) in parts.iter().enumerate() {
        let Some(uvs) = part.uvs else {
            continue;
        };
        for t in 0..part.triangles() {
            let Some((corners, positions)) = part.triangle(t) else {
                continue;
            };
            let [Some(t0), Some(t1), Some(t2)] =
                corners.map(|i| uvs.get(i).map(|&[u, v]| Vec2::new(u * s, v * s)))
            else {
                continue;
            };
            let spot =
                |k: [f32; 3]| positions[0] * k[0] + positions[1] * k[1] + positions[2] * k[2];
            match &field.surface {
                None => raster.triangle([t0, t1, t2], |k| spot(k).distance(field.origin)),
                Some((mesh, distances)) => {
                    let face = mesh.face(index, t);
                    if face.first().is_none_or(|&node| node == UNUSED) {
                        continue;
                    }
                    // The point's own triangle is reached straight from the point.
                    let home = field.home == (index, t);
                    raster.triangle([t0, t1, t2], |k| {
                        let spot = spot(k);
                        let start = if home {
                            spot.distance(field.origin)
                        } else {
                            f32::INFINITY
                        };
                        // Straight across the triangle from the nearest way in.
                        face.iter().fold(start, |best, &node| {
                            let way = distances[node as usize]
                                + mesh.positions[node as usize].distance(spot);
                            best.min(way)
                        })
                    });
                }
            }
        }
    }
    raster
        .values
        .iter()
        .zip(&raster.cover)
        .map(|(&d, &cover)| match cover {
            0 => 0,
            _ => ((d / range).clamp(0.0, 1.0) * 255.0).round() as u8,
        })
        .collect()
}

/// A square image of distances being filled triangle by triangle.
struct Raster {
    size: usize,
    values: Vec<f32>,
    /// Per texel: 0 not covered, 1 within a texel outside a triangle, 2 inside one.
    cover: Vec<u8>,
}

impl Raster {
    /// Fills the texels of the triangle with corners `t` (in texels; coordinates repeat) with
    /// `value` of their barycentric coordinates. A texel inside a triangle wins over one
    /// just outside another, and of two alike the smaller value.
    fn triangle(&mut self, [t0, t1, t2]: [Vec2; 3], value: impl Fn([f32; 3]) -> f32) {
        let size = self.size as i64;
        let (f1, f2) = (t1 - t0, t2 - t0);
        let area = f1.perp_dot(f2);
        if area.abs() < 1e-9 || !area.is_finite() {
            return;
        }
        let lo = t0.min(t1).min(t2) - 1.0;
        let hi = t0.max(t1).max(t2) + 1.0;
        let (x0, x1) = (lo.x.floor() as i64, hi.x.ceil() as i64);
        let (y0, y1) = (lo.y.floor() as i64, hi.y.ceil() as i64);
        // At most four wraps of the texture each way, as for painting.
        if (x1 - x0) > 4 * size || (y1 - y0) > 4 * size {
            return;
        }
        let lengths = [(t2 - t1).length(), (t0 - t2).length(), (t1 - t0).length()];
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let l1 = (c - t0).perp_dot(f2) / area;
                let l2 = f1.perp_dot(c - t0) / area;
                let ls = [1.0 - l1 - l2, l1, l2];
                let cover = if ls.iter().all(|&l| l >= 0.0) {
                    2
                } else if (0..3).all(|i| ls[i] * area.abs() / lengths[i].max(1e-6) >= -1.0) {
                    1
                } else {
                    continue;
                };
                let k = ls.map(|l| l.clamp(0.0, 1.0));
                let sum = (k[0] + k[1] + k[2]).max(1e-6);
                let v = value(k.map(|k| k / sum));
                let i = (y.rem_euclid(size) * size + x.rem_euclid(size)) as usize;
                if cover > self.cover[i] || (cover == self.cover[i] && v < self.values[i]) {
                    (self.cover[i], self.values[i]) = (cover, v);
                }
            }
        }
    }
}

/// The barycentric coordinates of `p`, a point in the plane of triangle `a b c`.
pub fn barycentric(p: Vec3, [a, b, c]: [Vec3; 3]) -> [f32; 3] {
    let (e1, e2, v) = (b - a, c - a, p - a);
    let (d11, d12, d22) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
    let (d1, d2) = (v.dot(e1), v.dot(e2));
    let det = d11 * d22 - d12 * d12;
    if det.abs() < 1e-20 {
        return [1.0, 0.0, 0.0];
    }
    let k1 = (d22 * d1 - d12 * d2) / det;
    let k2 = (d11 * d2 - d12 * d1) / det;
    [1.0 - k1 - k2, k1, k2]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip of `n` unit squares along x in the XY plane, two triangles each, its own
    /// four vertices per square (so squares meet only by position), mapped onto the texture
    /// left to right.
    fn strip(n: usize, at: Vec3) -> (Vec<Vec3>, Vec<usize>, Vec<[f32; 2]>) {
        let (mut positions, mut indices, mut uvs) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..n {
            let first = positions.len();
            for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                positions.push(at + Vec3::new(i as f32 + x, y, 0.0));
                uvs.push([(i as f32 + x) / n as f32, 1.0 - y]);
            }
            indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
        }
        (positions, indices, uvs)
    }

    fn part<'a>(id: u32, mesh: &'a (Vec<Vec3>, Vec<usize>, Vec<[f32; 2]>)) -> Part<'a> {
        Part {
            id: Some((id, 0)),
            positions: &mesh.0,
            indices: &mesh.1,
            uvs: Some(&mesh.2),
        }
    }

    #[test]
    fn locates_points_in_their_triangle() {
        let mesh = strip(2, Vec3::ZERO);
        let parts = [part(3, &mesh)];
        // The second triangle of the second square: corners 4, 6, 7.
        let point = Point {
            mesh: 3,
            primitive: 0,
            triangle: 3,
            at: [0.5, 0.5],
        };
        let (part, triangle, at) = locate(&parts, &point).unwrap();
        assert_eq!((part, triangle), (0, 3));
        assert_eq!(at, Vec3::new(1.5, 1.0, 0.0));
        assert!(locate(&parts, &Point { mesh: 4, ..point }).is_none());
        assert!(
            locate(
                &parts,
                &Point {
                    triangle: 9,
                    ..point
                }
            )
            .is_none()
        );
    }

    #[test]
    fn the_surface_way_goes_through_seams_and_around() {
        // A U: two strips of four squares, joined at their right ends by a third going up.
        // The left ends are 2 apart in a straight line and 4 + 2 + 4 along the surface.
        let low = strip(4, Vec3::ZERO);
        let high = strip(4, Vec3::new(0.0, 2.0, 0.0));
        let side = {
            let positions = vec![
                Vec3::new(4.0, 1.0, 0.0),
                Vec3::new(4.0, 2.0, 0.0),
                Vec3::new(4.0, 1.5, 1.0),
            ];
            (positions, vec![0, 1, 2], vec![[0.0, 0.0]; 3])
        };
        let parts = [part(0, &low), part(1, &high), part(2, &side)];
        let origin = Vec3::new(0.0, 0.0, 0.0);
        let field = measure(&parts, origin, (0, 0), Measure::Surface);
        let (mesh, distances) = field.surface.as_ref().unwrap();
        let node = mesh.face(1, 1)[2];
        assert_eq!(mesh.positions[node as usize], Vec3::new(0.0, 3.0, 0.0));
        // Across the low strip to (4, 1), up the side's edge to (4, 2) and across the high
        // strip: 2 × √17 + 1 at best, 10.4 along the triangles' edges alone. Squares meet
        // only by position.
        let d = distances[node as usize];
        assert!((9.24..9.5).contains(&d), "{d}");
        assert_eq!(field.farthest, d);
        let straight = measure(&parts, origin, (0, 0), Measure::Straight);
        assert_eq!(straight.farthest, 5.0);
    }

    #[test]
    fn parts_that_do_not_touch_are_bridged_by_their_nearest_vertices() {
        // Three strips in a row with gaps of 0.5 and 0.25, and a far one.
        let a = strip(1, Vec3::ZERO);
        let b = strip(1, Vec3::new(1.5, 0.0, 0.0));
        let c = strip(1, Vec3::new(2.75, 0.0, 0.0));
        let d = strip(1, Vec3::new(2.75, 40.0, 0.0));
        let parts = [part(0, &a), part(1, &b), part(2, &c), part(3, &d)];
        let field = measure(&parts, Vec3::ZERO, (0, 0), Measure::Surface);
        let (mesh, distances) = field.surface.as_ref().unwrap();
        // To the far strip's lower right corner: across the three strips and their gaps,
        // then 39 up; a little more or less by where the gaps are crossed.
        let node = mesh.face(3, 0)[1];
        assert_eq!(mesh.positions[node as usize], Vec3::new(3.75, 40.0, 0.0));
        let way = distances[node as usize];
        assert!((42.0..45.0).contains(&way), "{way}");
        assert!(distances.iter().all(|d| d.is_finite()));
        let mut gaps: Vec<f32> = mesh.gaps.iter().map(|gap| gap.2).collect();
        gaps.sort_by(f32::total_cmp);
        assert_eq!(gaps, [0.25, 0.5, 39.0]);
    }

    #[test]
    fn many_scattered_parts_all_get_a_distance() {
        // A grid of separate triangles, like leaf cards.
        let mut meshes = Vec::new();
        for i in 0..400 {
            let at = Vec3::new((i % 20) as f32 * 3.0, (i / 20) as f32 * 3.0, (i % 7) as f32);
            meshes.push((
                vec![at, at + Vec3::X, at + Vec3::Y],
                vec![0, 1, 2],
                vec![[0.0, 0.0]; 3],
            ));
        }
        let parts: Vec<Part> = meshes
            .iter()
            .enumerate()
            .map(|(i, m)| part(i as u32, m))
            .collect();
        let field = measure(&parts, Vec3::ZERO, (0, 0), Measure::Surface);
        let (mesh, distances) = field.surface.as_ref().unwrap();
        assert!(distances.iter().all(|d| d.is_finite()));
        // No shorter than the straight line.
        for (p, d) in mesh.positions.iter().zip(distances) {
            assert!(*d >= p.length() - 1e-3);
        }
        assert_eq!(mesh.gaps.len(), 399);
    }

    #[test]
    fn channels_fill_from_near_to_far() {
        let mesh = strip(4, Vec3::ZERO);
        let parts = [part(0, &mesh)];
        // The point: the middle of the left edge.
        let origin = Vec3::new(0.0, 0.5, 0.0);
        for kind in Measure::ALL {
            let field = measure(&parts, origin, (0, 1), kind);
            let values = rasterize(&parts, &field, 4.0, 16);
            assert_eq!(values.len(), 256);
            let row = &values[8 * 16..9 * 16];
            assert!(row[0] < 12, "{kind:?} {row:?}");
            assert!(row.windows(2).all(|w| w[0] <= w[1]), "{kind:?} {row:?}");
            // The right edge is 4 away: the whole range.
            assert!(row[15] > 235, "{kind:?} {row:?}");
            // Farthest: the corners, a little over 4 away, are 1.
            let auto = rasterize(&parts, &field, 0.0, 16);
            assert!(auto[15] >= 240 && auto[15] >= auto[8 * 16 + 15], "{kind:?}");
            // Half the range: the right half is all 1.
            let half = rasterize(&parts, &field, 2.0, 16);
            assert!(
                half[8 * 16 + 8..9 * 16].iter().all(|&v| v == 255),
                "{kind:?}"
            );
        }
        // No UVs, nothing filled.
        let bare = [Part {
            uvs: None,
            ..part(0, &mesh)
        }];
        let field = measure(&bare, origin, (0, 1), Measure::Straight);
        assert!(rasterize(&bare, &field, 0.0, 8).iter().all(|&v| v == 0));
    }

    #[test]
    fn barycentric_coordinates_weigh_the_corners() {
        let t = [Vec3::ZERO, Vec3::X * 2.0, Vec3::Y * 2.0];
        assert_eq!(barycentric(Vec3::ZERO, t), [1.0, 0.0, 0.0]);
        assert_eq!(barycentric(Vec3::new(1.0, 1.0, 0.0), t), [0.0, 0.5, 0.5]);
        assert_eq!(barycentric(Vec3::new(0.5, 0.0, 0.0), t), [0.75, 0.25, 0.0]);
    }
}
