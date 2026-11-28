//! The convex decomposition of [assetc](https://github.com/qbart/assetc): a model's triangles
//! as a few convex hulls that follow its shape, for a compound collider; and 3D convex hulls.
#![doc = include_str!("../README.md")]

use std::ops::RangeInclusive;

mod hull;
mod math;

pub use hull::{Hull, hull};

use hull::{Poly, quickhull};
use math::{AsDouble, Ext, Vec3};

/// The range of [`Settings::max_hulls`].
pub const MAX_HULLS: RangeInclusive<u32> = 1..=256;
/// The range of [`Settings::max_vertices`].
pub const MAX_VERTICES: RangeInclusive<u32> = 4..=255;
/// The range of [`Settings::error`].
pub const ERROR: RangeInclusive<f32> = 0.001..=0.5;
/// The range of [`Settings::resolution`].
pub const RESOLUTION: RangeInclusive<u32> = 16..=256;

/// How a model is decomposed; see docs/features/colliders.md#settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Most hulls in the collider.
    pub max_hulls: u32,
    /// Most corner points of one hull.
    pub max_vertices: u32,
    /// Most volume a hull may add over the part of the model it covers, as a share of the
    /// model's volume.
    pub error: f32,
    /// Voxels along the model's longest side.
    pub resolution: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_hulls: 32,
            max_vertices: 32,
            error: 0.01,
            resolution: 64,
        }
    }
}

impl Settings {
    /// The first setting out of its range, as `max_hulls must be 1 to 256, not 300`.
    pub fn validate(&self) -> Result<(), String> {
        fn check<T: PartialOrd + std::fmt::Display>(
            key: &str,
            v: T,
            range: RangeInclusive<T>,
        ) -> Result<(), String> {
            if range.contains(&v) {
                Ok(())
            } else {
                Err(format!(
                    "{key} must be {} to {}, not {v}",
                    range.start(),
                    range.end()
                ))
            }
        }
        check("max_hulls", self.max_hulls, MAX_HULLS)?;
        check("max_vertices", self.max_vertices, MAX_VERTICES)?;
        check("error", self.error, ERROR)?;
        check("resolution", self.resolution, RESOLUTION)
    }
}

/// One convex piece of a collider, in the model's space.
#[derive(Debug, Clone, PartialEq)]
pub struct Convex {
    pub hull: Hull,
    pub volume: f32,
    /// Center of its volume.
    pub center: [f32; 3],
    /// Its box.
    pub min: [f32; 3],
    pub max: [f32; 3],
}

/// A model as convex hulls; see docs/features/colliders.md.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Collider {
    /// Largest first.
    pub hulls: Vec<Convex>,
    /// The model's volume as voxelized (its inside filled when it is closed).
    pub model_volume: f32,
}

impl Collider {
    /// Corner points of every hull.
    pub fn points(&self) -> usize {
        self.hulls.iter().map(|h| h.hull.points.len()).sum()
    }

    /// Triangles of every hull.
    pub fn faces(&self) -> usize {
        self.hulls.iter().map(|h| h.hull.faces.len()).sum()
    }

    /// The hulls' volume, summed.
    pub fn volume(&self) -> f32 {
        self.hulls.iter().map(|h| h.volume).sum()
    }

    /// The box around every hull (zero without hulls).
    pub fn bounds(&self) -> [[f32; 3]; 2] {
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for h in &self.hulls {
            (min, max) = (min.min(h.min.into()), max.max(h.max.into()));
        }
        if self.hulls.is_empty() {
            [[0.0; 3]; 2]
        } else {
            [min.to_array(), max.to_array()]
        }
    }
}

/// Decomposes the triangles (`positions` indexed by `triangles`) into convex hulls; see
/// docs/features/colliders.md#how-it-is-made.
pub fn decompose(
    positions: &[[f32; 3]],
    triangles: &[[u32; 3]],
    settings: &Settings,
) -> Result<Collider, String> {
    settings.validate()?;
    let positions: Vec<Vec3> = positions.iter().map(|&p| Vec3::from(p)).collect();
    let positions = positions.as_slice();
    let grid = Grid::new(positions, triangles, settings.resolution)?;
    let solid: Voxels = grid.solid_voxels();
    let total = solid.len() as f32;
    // What the hulls may add in all, in voxels.
    let budget = settings.error * total;
    let mut marks = Marks::new(grid.len());

    // Split the part whose hull holds the most empty voxels, until the hulls hold no more
    // than the budget or there are twice the hulls wanted (merging brings them back down).
    let limit = if settings.max_hulls <= 1 {
        1
    } else {
        settings.max_hulls as usize * 2
    };
    // Cuts move to a corner of the model's within two voxels, so hulls end where its faces do.
    let mut coords: [Vec<f32>; 3] = Default::default();
    for t in triangles {
        for &i in t {
            for (k, list) in coords.iter_mut().enumerate() {
                list.push(positions[i as usize][k]);
            }
        }
    }
    for list in &mut coords {
        list.sort_by(f32::total_cmp);
        list.dedup();
    }
    let snap = |axis: usize, x: f32| {
        let list = &coords[axis];
        let i = list.partition_point(|&c| c < x);
        [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter_map(|i| list.get(i).copied())
            .filter(|c| (c - x).abs() <= 2.0 * grid.h)
            .min_by(|a, b| (a - x).abs().total_cmp(&(b - x).abs()))
            .unwrap_or(x)
    };
    let everywhere = [Vec3::splat(f32::NEG_INFINITY), Vec3::splat(f32::INFINITY)];
    let mut parts: Vec<Part> = marks
        .components(&grid, solid)
        .into_iter()
        .map(|v| Part::new(v, grid.n, everywhere))
        .collect();
    while parts.len() < limit && parts.iter().map(|p| p.extra).sum::<f32>() > budget {
        let Some(i) = (0..parts.len())
            .filter(|&i| !parts[i].done && parts[i].extra > 0.0)
            .max_by(|&a, &b| parts[a].extra.total_cmp(&parts[b].extra))
        else {
            break;
        };
        let Some((axis, cut)) = best_cut(&parts[i].voxels, grid.n) else {
            parts[i].done = true;
            continue;
        };
        let part = parts.swap_remove(i);
        let (left, right): (Voxels, Voxels) = part.voxels.iter().partition(|v| v[axis] < cut);
        let plane = snap(axis, grid.origin[axis] + cut as f32 * grid.h);
        let (mut below, mut above) = (part.region, part.region);
        below[1][axis] = plane;
        above[0][axis] = plane;
        for (side, region) in [(left, below), (right, above)] {
            for voxels in marks.components(&grid, side) {
                parts.push(Part::new(voxels, grid.n, region));
            }
        }
    }

    // Each part's hull around the model's own surface inside it.
    let mut labels = vec![u32::MAX; grid.len()];
    for (k, part) in parts.iter().enumerate() {
        for v in &part.voxels {
            labels[grid.at(*v)] = k as u32;
        }
    }
    let boxes: Vec<[Vec3; 2]> = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            [a.min(b).min(c), a.max(b).max(c)]
        })
        .collect();
    let max_vertices = settings.max_vertices.max(4) as usize;
    let voxel = grid.h * grid.h * grid.h;
    let mut pieces: Vec<Piece> = parts
        .iter()
        .enumerate()
        .filter_map(|(k, part)| {
            let points = surface_hull(
                &grid,
                &labels,
                k as u32,
                part,
                (positions, triangles, &boxes),
            )
            .or_else(|| {
                let corners = voxel_corners(&part.voxels, grid.n);
                quickhull(&corners.iter().map(|&p| grid.world(p)).collect::<Vec<_>>())
            })?
            .points;
            let mut rows = Rows::new(grid.n);
            for v in &part.voxels {
                rows.add(*v);
            }
            Some(Piece::new(points, max_vertices, rows))
        })
        .collect();

    // Merge the pair whose hull grows the least, while there are too many hulls or a merge
    // stays within the budget.
    merge(
        &mut pieces,
        (settings.max_hulls.max(1) as usize, max_vertices),
        budget,
    );

    let mut hulls: Vec<Convex> = pieces
        .into_iter()
        .filter_map(|piece| convex(quickhull(&piece.simple)?))
        .collect();
    hulls.sort_by(|a, b| b.volume.total_cmp(&a.volume));
    Ok(Collider {
        hulls,
        model_volume: total * voxel,
    })
}

/// Voxel coordinates.
type Voxels = Vec<[u16; 3]>;

/// The model's voxels: those its surface passes through and, for a closed surface, those
/// inside it. One empty voxel pads every side.
struct Grid {
    n: [usize; 3],
    origin: Vec3,
    /// Voxel size.
    h: f32,
    solid: Vec<bool>,
}

impl Grid {
    fn new(positions: &[Vec3], triangles: &[[u32; 3]], resolution: u32) -> Result<Self, String> {
        let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for t in triangles {
            for &i in t {
                let p = *positions
                    .get(i as usize)
                    .ok_or("a triangle uses a vertex the model does not have")?;
                (min, max) = (min.min(p), max.max(p));
            }
        }
        let size = max - min;
        let longest = size.max_element();
        if !longest.is_finite() || longest <= 0.0 {
            return Err("the model has no triangles to make a collider of".into());
        }
        let resolution = resolution.max(1) as usize;
        let h = longest / resolution as f32;
        let cells = |s: f32| ((s / h).ceil() as usize).clamp(1, resolution) + 2;
        let n = [cells(size.x), cells(size.y), cells(size.z)];
        let origin = (min + max) / 2.0 - Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32) * h / 2.0;
        let len = n[0] * n[1] * n[2];
        let at = |[x, y, z]: [usize; 3]| x + n[0] * (y + n[1] * z);

        let mut surface = vec![false; len];
        for t in triangles {
            let tri = t.map(|i| (positions[i as usize] - origin) / h);
            let lo = tri[0].min(tri[1]).min(tri[2]).floor().max(Vec3::ZERO);
            let hi = tri[0].max(tri[1]).max(tri[2]).floor();
            let range = |k: usize| lo[k] as usize..=(hi[k].max(0.0) as usize).min(n[k] - 1);
            for z in range(2) {
                for y in range(1) {
                    for x in range(0) {
                        let center = Vec3::new(x as f32, y as f32, z as f32) + 0.5;
                        if !surface[at([x, y, z])] && overlaps(center, 0.5 + 1e-3, tri) {
                            surface[at([x, y, z])] = true;
                        }
                    }
                }
            }
        }

        // Everything the outside does not reach is solid.
        let mut outside = vec![false; len];
        let mut stack = vec![0];
        outside[0] = true;
        while let Some(i) = stack.pop() {
            let v = [i % n[0], (i / n[0]) % n[1], i / (n[0] * n[1])];
            for k in 0..3 {
                for up in [false, true] {
                    let mut w = v;
                    if up {
                        if w[k] + 1 >= n[k] {
                            continue;
                        }
                        w[k] += 1;
                    } else {
                        if w[k] == 0 {
                            continue;
                        }
                        w[k] -= 1;
                    }
                    let j = at(w);
                    if !outside[j] && !surface[j] {
                        outside[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        Ok(Self {
            n,
            origin,
            h,
            solid: outside.into_iter().map(|o| !o).collect(),
        })
    }

    fn len(&self) -> usize {
        self.solid.len()
    }

    fn at(&self, [x, y, z]: [u16; 3]) -> usize {
        x as usize + self.n[0] * (y as usize + self.n[1] * z as usize)
    }

    /// A point in voxel coordinates in the model's space.
    fn world(&self, p: [u16; 3]) -> Vec3 {
        self.origin + Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) * self.h
    }

    fn solid_voxels(&self) -> Voxels {
        let mut out = Vec::new();
        for z in 0..self.n[2] {
            for y in 0..self.n[1] {
                for x in 0..self.n[0] {
                    let v = [x as u16, y as u16, z as u16];
                    if self.solid[self.at(v)] {
                        out.push(v);
                    }
                }
            }
        }
        out
    }
}

/// Whether the triangle `tri` touches the cube at `center` with half side `half`
/// (separating axes).
fn overlaps(center: Vec3, half: f32, tri: [Vec3; 3]) -> bool {
    let v = tri.map(|p| p - center);
    let range = |axis: Vec3| {
        let d = v.map(|p| axis.dot(p));
        (d[0].min(d[1]).min(d[2]), d[0].max(d[1]).max(d[2]))
    };
    let separated = |axis: Vec3| {
        let r = half * axis.abs().sum();
        let (lo, hi) = range(axis);
        lo > r || hi < -r
    };
    if [Vec3::X, Vec3::Y, Vec3::Z].into_iter().any(separated) {
        return false;
    }
    let edges = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    let normal = edges[0].cross(edges[1]);
    if normal.length2() > 0.0 && separated(normal) {
        return false;
    }
    for edge in edges {
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let axis = axis.cross(edge);
            if axis.length2() > 1e-12 && separated(axis) {
                return false;
            }
        }
    }
    true
}

/// Marks voxels of one part at a time, to find its connected pieces.
struct Marks {
    marks: Vec<u32>,
    stamp: u32,
}

impl Marks {
    fn new(len: usize) -> Self {
        Self {
            marks: vec![0; len],
            stamp: 0,
        }
    }

    /// `voxels` in pieces that touch (by a face, an edge or a corner).
    fn components(&mut self, grid: &Grid, voxels: Voxels) -> Vec<Voxels> {
        self.stamp += 2;
        let (member, seen) = (self.stamp - 1, self.stamp);
        for v in &voxels {
            self.marks[grid.at(*v)] = member;
        }
        let mut out = Vec::new();
        for &start in &voxels {
            if self.marks[grid.at(start)] != member {
                continue;
            }
            self.marks[grid.at(start)] = seen;
            let mut piece = vec![start];
            let mut next = 0;
            while next < piece.len() {
                let v = piece[next];
                next += 1;
                for dz in -1i32..=1 {
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let w = [v[0] as i32 + dx, v[1] as i32 + dy, v[2] as i32 + dz];
                            if (0..3).any(|k| w[k] < 0 || w[k] >= grid.n[k] as i32) {
                                continue;
                            }
                            let w = w.map(|c| c as u16);
                            let j = grid.at(w);
                            if self.marks[j] == member {
                                self.marks[j] = seen;
                                piece.push(w);
                            }
                        }
                    }
                }
            }
            out.push(piece);
        }
        out
    }
}

/// Part of the model while splitting: its voxels and how much its hull adds to them.
struct Part {
    voxels: Voxels,
    /// The empty voxels inside their hull.
    extra: f32,
    /// No cut makes it smaller.
    done: bool,
    /// The box the cuts so far leave it, in the model's space.
    region: [Vec3; 2],
}

impl Part {
    fn new(voxels: Voxels, n: [usize; 3], region: [Vec3; 2]) -> Self {
        let mut rows = Rows::new(n);
        for v in &voxels {
            rows.add(*v);
        }
        Self {
            extra: rows.extra(),
            voxels,
            done: false,
            region,
        }
    }
}

/// The first and last voxel of each row along X: the hull of a set of voxels is the hull of
/// these rows' ends.
struct Rows {
    n: [usize; 3],
    lo: Vec<u16>,
    hi: Vec<u16>,
    count: usize,
}

impl Rows {
    fn new(n: [usize; 3]) -> Self {
        Self {
            n,
            lo: vec![u16::MAX; n[1] * n[2]],
            hi: vec![0; n[1] * n[2]],
            count: 0,
        }
    }

    fn add(&mut self, [x, y, z]: [u16; 3]) {
        let row = y as usize + self.n[1] * z as usize;
        self.lo[row] = self.lo[row].min(x);
        self.hi[row] = self.hi[row].max(x);
        self.count += 1;
    }

    /// Both sets' rows.
    fn joined(&self, other: &Self) -> Self {
        Self {
            n: self.n,
            lo: self
                .lo
                .iter()
                .zip(&other.lo)
                .map(|(a, b)| *a.min(b))
                .collect(),
            hi: self
                .hi
                .iter()
                .zip(&other.hi)
                .map(|(a, b)| *a.max(b))
                .collect(),
            count: self.count + other.count,
        }
    }

    /// The corners of the rows' end voxels, in voxel coordinates.
    fn corners(&self) -> Vec<[u16; 3]> {
        let mut out = Vec::new();
        for (row, (&lo, &hi)) in self.lo.iter().zip(&self.hi).enumerate() {
            if lo == u16::MAX {
                continue;
            }
            let (y, z) = ((row % self.n[1]) as u16, (row / self.n[1]) as u16);
            for x in [lo, hi + 1] {
                for (dy, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    out.push([x, y + dy, z + dz]);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The empty voxels inside the hull of the voxels: those whose center is inside the hull
    /// of their centers (of their corners, when the centers are flat).
    fn extra(&self) -> f32 {
        if self.count == 0 {
            return 0.0;
        }
        let mut centers = Vec::new();
        for (row, (&lo, &hi)) in self.lo.iter().zip(&self.hi).enumerate() {
            if lo == u16::MAX {
                continue;
            }
            let (y, z) = ((row % self.n[1]) as f32, (row / self.n[1]) as f32);
            centers.push(Vec3::new(lo as f32, y, z));
            if hi != lo {
                centers.push(Vec3::new(hi as f32, y, z));
            }
        }
        let Some(hull) = quickhull(&centers).or_else(|| {
            let corners: Vec<Vec3> = self
                .corners()
                .into_iter()
                .map(|p| Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) - 0.5)
                .collect();
            quickhull(&corners)
        }) else {
            return 0.0;
        };
        (lattice_points(&hull) as f32 - self.count as f32).max(0.0)
    }
}

/// The points with whole coordinates inside or on `hull`.
fn lattice_points(hull: &Poly) -> usize {
    const EPS: f64 = 1e-4;
    let planes: Vec<(math::DVec3, f64)> = hull
        .faces
        .iter()
        .map(|&[a, b, c]| {
            let [a, b, c] = [a, b, c].map(|i| hull.points[i].as_dvec3());
            let n = (b - a).cross(c - a).normalize_or_zero();
            (n, n.dot(a))
        })
        .collect();
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &p in &hull.points {
        (min, max) = (min.min(p), max.max(p));
    }
    let mut count = 0;
    for z in (min.z as f64 - EPS).ceil() as i64..=(max.z as f64 + EPS).floor() as i64 {
        for y in (min.y as f64 - EPS).ceil() as i64..=(max.y as f64 + EPS).floor() as i64 {
            let (mut lo, mut hi) = (min.x as f64 - 1.0, max.x as f64 + 1.0);
            for &(n, d) in &planes {
                let r = d - n.y * y as f64 - n.z * z as f64;
                if n.x.abs() < 1e-9 {
                    if r < -EPS {
                        hi = lo - 1.0;
                    }
                } else if n.x > 0.0 {
                    hi = hi.min(r / n.x);
                } else {
                    lo = lo.max(r / n.x);
                }
            }
            let (lo, hi) = ((lo - EPS).ceil() as i64, (hi + EPS).floor() as i64);
            if hi >= lo {
                count += (hi - lo + 1) as usize;
            }
        }
    }
    count
}

fn voxel_corners(voxels: &[[u16; 3]], n: [usize; 3]) -> Vec<[u16; 3]> {
    let mut rows = Rows::new(n);
    for v in voxels {
        rows.add(*v);
    }
    rows.corners()
}

/// The cut, along an axis between two layers of voxels, after which the two sides' hulls add
/// the least: the axis and the first layer of the far side. `None` for a single voxel.
fn best_cut(voxels: &[[u16; 3]], n: [usize; 3]) -> Option<(usize, u16)> {
    const STEPS: usize = 16;
    let (mut lo, mut hi) = ([u16::MAX; 3], [0u16; 3]);
    for v in voxels {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let mut cuts = Vec::new();
    let mut steps = [1u16; 3];
    for axis in 0..3 {
        let span = hi[axis] - lo[axis];
        if span == 0 {
            continue;
        }
        steps[axis] = (span as usize).div_ceil(STEPS) as u16;
        cuts.extend(
            (lo[axis] + 1..=hi[axis])
                .step_by(steps[axis] as usize)
                .map(|c| (axis, c)),
        );
    }
    let costs = cut_costs(voxels, n, &cuts);
    let best = (0..cuts.len()).min_by(|&a, &b| costs[a].total_cmp(&costs[b]))?;
    let (axis, cut) = cuts[best];
    let mut tried: Vec<(u16, f32)> = cuts
        .iter()
        .zip(&costs)
        .filter(|((a, _), _)| *a == axis)
        .map(|((_, c), cost)| (*c, *cost))
        .collect();
    // Then every layer around the best.
    let step = steps[axis];
    if step > 1 {
        let finer: Vec<(usize, u16)> = (cut.saturating_sub(step - 1).max(lo[axis] + 1)
            ..=(cut + step - 1).min(hi[axis]))
            .filter(|&c| c != cut)
            .map(|c| (axis, c))
            .collect();
        let finer_costs = cut_costs(voxels, n, &finer);
        tried.extend(finer.iter().map(|(_, c)| *c).zip(finer_costs));
    }
    // Of cuts that are as good, the middle one: in a gap, halfway across it.
    let least = tried.iter().map(|t| t.1).fold(f32::INFINITY, f32::min);
    let mut equal: Vec<u16> = tried
        .iter()
        .filter(|t| t.1 <= least + 0.5)
        .map(|t| t.0)
        .collect();
    equal.sort_unstable();
    Some((axis, equal[equal.len() / 2]))
}

/// What each cut's two sides' hulls add, computed on every core.
fn cut_costs(voxels: &[[u16; 3]], n: [usize; 3], cuts: &[(usize, u16)]) -> Vec<f32> {
    let cost = |&(axis, cut): &(usize, u16)| {
        let (mut left, mut right) = (Rows::new(n), Rows::new(n));
        for v in voxels {
            if v[axis] < cut {
                left.add(*v);
            } else {
                right.add(*v);
            }
        }
        left.extra() + right.extra()
    };
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(cuts.len().max(1));
    if threads <= 1 {
        return cuts.iter().map(cost).collect();
    }
    let chunk = cuts.len().div_ceil(threads);
    std::thread::scope(|s| {
        let handles: Vec<_> = cuts
            .chunks(chunk)
            .map(|chunk| s.spawn(move || chunk.iter().map(cost).collect::<Vec<f32>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a cut cost thread"))
            .collect()
    })
}

/// The hull of the model's surface inside part `label`: its triangles clipped to the part's
/// box, those that lie in its voxels (or next to them). `None` when that makes no volume.
fn surface_hull(
    grid: &Grid,
    labels: &[u32],
    label: u32,
    part: &Part,
    (positions, triangles, boxes): (&[Vec3], &[[u32; 3]], &[[Vec3; 2]]),
) -> Option<Poly> {
    let (mut lo, mut hi) = ([u16::MAX; 3], [0u16; 3]);
    for v in &part.voxels {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    // Its box, out to the planes it was cut along (two voxels off at most).
    let (min, max) = (
        (grid.world(lo) - 2.0 * grid.h).max(part.region[0]),
        (grid.world(hi.map(|c| c + 1)) + 2.0 * grid.h).min(part.region[1]),
    );
    let mut points = Vec::new();
    for (t, b) in triangles.iter().zip(boxes) {
        if (0..3).any(|k| b[0][k] > max[k] || b[1][k] < min[k]) {
            continue;
        }
        let polygon = clip(t.map(|i| positions[i as usize]).to_vec(), min, max);
        if polygon.is_empty() {
            continue;
        }
        let center = polygon.iter().sum::<Vec3>() / polygon.len() as f32;
        let v = ((center - grid.origin) / grid.h).floor();
        let mine = (-1i32..=1).any(|dz| {
            (-1i32..=1).any(|dy| {
                (-1i32..=1).any(|dx| {
                    let w = [v.x as i32 + dx, v.y as i32 + dy, v.z as i32 + dz];
                    (0..3).all(|k| w[k] >= 0 && w[k] < grid.n[k] as i32)
                        && labels[grid.at(w.map(|c| c as u16))] == label
                })
            })
        });
        if mine {
            points.extend(polygon);
        }
    }
    quickhull(&points).filter(|h| h.volume() > 0.0)
}

/// `polygon` clipped to the box `min`–`max`.
fn clip(mut polygon: Vec<Vec3>, min: Vec3, max: Vec3) -> Vec<Vec3> {
    for k in 0..3 {
        for (bound, keep_below) in [(min[k], false), (max[k], true)] {
            if polygon.is_empty() {
                return polygon;
            }
            let inside = |p: Vec3| {
                if keep_below {
                    p[k] <= bound
                } else {
                    p[k] >= bound
                }
            };
            let mut out = Vec::with_capacity(polygon.len() + 2);
            for i in 0..polygon.len() {
                let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
                if inside(a) {
                    out.push(a);
                }
                if inside(a) != inside(b) {
                    let t = (bound - a[k]) / (b[k] - a[k]);
                    let mut p = a + (b - a) * t;
                    p[k] = bound;
                    out.push(p);
                }
            }
            polygon = out;
        }
    }
    polygon
}

/// A hull while merging: its corners, the at most `max_vertices` of them it keeps, and the
/// model's volume inside it.
struct Piece {
    points: Vec<Vec3>,
    simple: Vec<Vec3>,
    /// Of the hull of `simple`.
    volume: f32,
    /// The voxels it covers, as rows.
    rows: Rows,
    /// The empty voxels inside their hull.
    extra: f32,
}

impl Piece {
    fn new(points: Vec<Vec3>, max_vertices: usize, rows: Rows) -> Self {
        let simple = simplify(&points, max_vertices);
        let volume = quickhull(&simple).map_or(0.0, |h| h.volume());
        Self {
            points,
            simple,
            volume,
            extra: rows.extra(),
            rows,
        }
    }
}

/// The volume of the hull of two pieces.
fn joint_volume(a: &Piece, b: &Piece) -> f32 {
    let mut both = a.simple.clone();
    both.extend(&b.simple);
    quickhull(&both).map_or(0.0, |h| h.volume())
}

/// Merges pieces two at a time: while there are more than `max_hulls`, the two whose joint
/// hull grows the least; then, while one does, two whose merge keeps the empty voxels inside
/// all hulls within `budget`.
fn merge(pieces: &mut Vec<Piece>, (max_hulls, max_vertices): (usize, usize), budget: f32) {
    /// Pairs tried for a merge that fits, least growing first.
    const TRIES: usize = 8;
    // The joint volume of each pair `i < j`, in `joint[i][j]`.
    let mut joint: Vec<Vec<f32>> = {
        let pieces = &*pieces;
        let row = |i: usize| -> Vec<f32> {
            (0..pieces.len())
                .map(|j| {
                    if j > i {
                        joint_volume(&pieces[i], &pieces[j])
                    } else {
                        f32::INFINITY
                    }
                })
                .collect()
        };
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(pieces.len().max(1));
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    s.spawn(move || {
                        (t..pieces.len())
                            .step_by(threads)
                            .map(|i| (i, row(i)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            let mut rows = vec![Vec::new(); pieces.len()];
            for h in handles {
                for (i, r) in h.join().expect("a merge cost thread") {
                    rows[i] = r;
                }
            }
            rows
        })
    };
    let mut alive = vec![true; pieces.len()];
    let mut count = pieces.len();
    while count > 1 {
        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();
        for i in (0..pieces.len()).filter(|&i| alive[i]) {
            for j in (i + 1..pieces.len()).filter(|&j| alive[j]) {
                let grows = joint[i][j] - pieces[i].volume - pieces[j].volume;
                pairs.push((i, j, grows));
            }
        }
        pairs.sort_by(|a, b| a.2.total_cmp(&b.2));
        let spent: f32 = (0..pieces.len())
            .filter(|&k| alive[k])
            .map(|k| pieces[k].extra)
            .sum();
        let best = if count > max_hulls {
            pairs.first().copied()
        } else {
            pairs.iter().take(TRIES).copied().find(|&(i, j, _)| {
                let joined = pieces[i].rows.joined(&pieces[j].rows).extra();
                spent - pieces[i].extra - pieces[j].extra + joined <= budget
            })
        };
        let Some((i, j, _)) = best else { break };
        let mut points = std::mem::take(&mut pieces[i].points);
        points.extend(std::mem::take(&mut pieces[j].points));
        let points = quickhull(&points).map_or(points, |h| h.points);
        let rows = pieces[i].rows.joined(&pieces[j].rows);
        pieces[i] = Piece::new(points, max_vertices, rows);
        alive[j] = false;
        count -= 1;
        for k in (0..pieces.len()).filter(|&k| alive[k] && k != i) {
            joint[i.min(k)][i.max(k)] = joint_volume(&pieces[i], &pieces[k]);
        }
    }
    let mut k = 0;
    pieces.retain(|_| {
        k += 1;
        alive[k - 1]
    });
}

/// At most `max` of `points` whose hull keeps as much of theirs as it can: from a first
/// tetrahedron, the point farthest outside the hull so far, again and again.
fn simplify(points: &[Vec3], max: usize) -> Vec<Vec3> {
    let Some(full) = quickhull(points) else {
        return points.to_vec();
    };
    if full.points.len() <= max {
        return full.points;
    }
    let points = full.points;
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &p in &points {
        (lo, hi) = (lo.min(p), hi.max(p));
    }
    let eps = (hi - lo).max_element() * 1e-5;
    let axis = (hi - lo).max_position();
    let by = |f: &dyn Fn(Vec3) -> f32| {
        (0..points.len())
            .max_by(|&a, &b| f(points[a]).total_cmp(&f(points[b])))
            .unwrap_or(0)
    };
    let a = by(&|p| -p[axis]);
    let b = by(&|p| p[axis]);
    let line = (points[b] - points[a]).normalize_or_zero();
    let c = by(&|p| {
        let d = p - points[a];
        (d - line * d.dot(line)).length()
    });
    let normal = (points[b] - points[a])
        .cross(points[c] - points[a])
        .normalize_or_zero();
    let d = by(&|p| normal.dot(p - points[a]).abs());
    let mut chosen = vec![a, b, c, d];
    chosen.dedup();
    while chosen.len() < max {
        let Some(current) = quickhull(&chosen.iter().map(|&i| points[i]).collect::<Vec<_>>())
        else {
            break;
        };
        let planes: Vec<(Vec3, f32)> = current
            .faces
            .iter()
            .map(|&[a, b, c]| {
                let [a, b, c] = [a, b, c].map(|i| current.points[i]);
                let n = (b - a).cross(c - a).normalize_or_zero();
                (n, n.dot(a))
            })
            .collect();
        let outside = |p: Vec3| {
            planes
                .iter()
                .map(|(n, d)| n.dot(p) - d)
                .fold(f32::NEG_INFINITY, f32::max)
        };
        let far = (0..points.len())
            .filter(|i| !chosen.contains(i))
            .max_by(|&x, &y| outside(points[x]).total_cmp(&outside(points[y])));
        match far {
            Some(i) if outside(points[i]) > eps => chosen.push(i),
            _ => break,
        }
    }
    let kept: Vec<Vec3> = chosen.iter().map(|&i| points[i]).collect();
    quickhull(&kept).map_or(kept, |h| h.points)
}

fn convex(hull: Poly) -> Option<Convex> {
    let o = *hull.points.first()?;
    let (mut volume, mut moment) = (0.0, Vec3::ZERO);
    for &[a, b, c] in &hull.faces {
        let [a, b, c] = [a, b, c].map(|i| hull.points[i] - o);
        let v = a.dot(b.cross(c)) / 6.0;
        volume += v;
        moment += (a + b + c) / 4.0 * v;
    }
    if volume <= 0.0 {
        return None;
    }
    let (mut min, mut max) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for &p in &hull.points {
        (min, max) = (min.min(p), max.max(p));
    }
    Some(Convex {
        center: (o + moment / volume).to_array(),
        volume,
        min: min.to_array(),
        max: max.to_array(),
        hull: Hull::from(&hull),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An axis-aligned box's 12 triangles, facing out.
    fn cuboid(min: Vec3, max: Vec3, positions: &mut Vec<[f32; 3]>, triangles: &mut Vec<[u32; 3]>) {
        let base = positions.len() as u32;
        for i in 0..8 {
            positions.push([
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            ]);
        }
        for [a, b, c] in [
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ] {
            triangles.push([base + a, base + b, base + c]);
        }
    }

    /// An L of two boxes, `2 × 1 × 1` and `1 × 2 × 1` from the origin.
    fn l_shape() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        let (mut p, mut t) = (Vec::new(), Vec::new());
        cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 1.0), &mut p, &mut t);
        cuboid(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 3.0, 1.0),
            &mut p,
            &mut t,
        );
        (p, t)
    }

    fn inside(c: &Collider, p: Vec3) -> bool {
        c.hulls.iter().any(|h| {
            h.hull.faces.iter().all(|&[a, b, cc]| {
                let [a, b, cc] = [a, b, cc].map(|i| Vec3::from(h.hull.points[i]));
                (b - a).cross(cc - a).normalize().dot(p - a) <= 1e-3
            })
        })
    }

    #[test]
    fn a_box_is_one_hull_of_its_corners() {
        let (mut p, mut t) = (Vec::new(), Vec::new());
        cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5), &mut p, &mut t);
        let c = decompose(&p, &t, &Settings::default()).unwrap();
        assert_eq!(c.hulls.len(), 1);
        let h = &c.hulls[0];
        assert_eq!(h.hull.points.len(), 8);
        assert!((h.volume - 1.0).abs() < 1e-4, "{}", h.volume);
        assert!((Vec3::from(h.center) - Vec3::new(1.0, 0.5, 0.25)).length() < 1e-4);
        assert_eq!(c.bounds(), [[0.0; 3], [2.0, 1.0, 0.5]]);
        // Filled: the voxels inside count.
        assert!((c.model_volume - 1.0).abs() < 0.3, "{}", c.model_volume);
    }

    #[test]
    fn an_l_is_two_hulls_that_follow_it() {
        let (p, t) = l_shape();
        let c = decompose(&p, &t, &Settings::default()).unwrap();
        assert_eq!(
            c.hulls.len(),
            2,
            "{:?}",
            c.hulls.iter().map(|h| h.volume).collect::<Vec<_>>()
        );
        // Close to the L's own volume (4), not its box's (6).
        assert!((c.volume() - 4.0).abs() < 0.25, "{}", c.volume());
        assert!(inside(&c, Vec3::new(1.5, 0.5, 0.5)));
        assert!(inside(&c, Vec3::new(0.5, 2.5, 0.5)));
        assert!(!inside(&c, Vec3::new(1.6, 2.0, 0.5)));
    }

    #[test]
    fn limits_hulls_and_vertices() {
        // A comb: five teeth on a bar.
        let (mut p, mut t) = (Vec::new(), Vec::new());
        cuboid(Vec3::ZERO, Vec3::new(9.0, 1.0, 1.0), &mut p, &mut t);
        for i in 0..5 {
            let x = i as f32 * 2.0;
            cuboid(
                Vec3::new(x, 1.0, 0.0),
                Vec3::new(x + 1.0, 4.0, 1.0),
                &mut p,
                &mut t,
            );
        }
        let all = decompose(&p, &t, &Settings::default()).unwrap();
        assert!(all.hulls.len() >= 6, "{}", all.hulls.len());
        assert!(all.volume() < 25.0, "{}", all.volume());
        let few = Settings {
            max_hulls: 3,
            max_vertices: 6,
            ..Settings::default()
        };
        let c = decompose(&p, &t, &few).unwrap();
        assert_eq!(c.hulls.len(), 3);
        assert!(c.hulls.iter().all(|h| h.hull.points.len() <= 6));
        let one = Settings {
            max_hulls: 1,
            ..Settings::default()
        };
        assert_eq!(decompose(&p, &t, &one).unwrap().hulls.len(), 1);
    }

    #[test]
    fn separate_pieces_stay_separate() {
        let (mut p, mut t) = (Vec::new(), Vec::new());
        cuboid(Vec3::ZERO, Vec3::ONE, &mut p, &mut t);
        cuboid(
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(4.0, 1.0, 1.0),
            &mut p,
            &mut t,
        );
        let c = decompose(&p, &t, &Settings::default()).unwrap();
        assert_eq!(c.hulls.len(), 2);
        assert!((c.volume() - 2.0).abs() < 1e-3, "{}", c.volume());
        // Made one, they become their joint hull.
        let one = Settings {
            max_hulls: 1,
            ..Settings::default()
        };
        let c = decompose(&p, &t, &one).unwrap();
        assert!((c.volume() - 4.0).abs() < 1e-3, "{}", c.volume());
    }

    #[test]
    fn a_flat_model_is_a_thin_slab() {
        let p = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0]];
        let t = vec![[0, 2, 1], [1, 2, 3]];
        let c = decompose(&p, &t, &Settings::default()).unwrap();
        assert_eq!(c.hulls.len(), 1);
        let [min, max] = c.bounds();
        assert!(max[1] - min[1] > 0.0 && max[1] - min[1] <= 2.0 / 64.0 + 1e-4);
        assert!(min[0] <= 0.0 && max[0] >= 1.0);
    }

    #[test]
    fn lattice_points_of_a_hull() {
        let cube: Vec<Vec3> = (0..8)
            .map(|i| {
                Vec3::new(
                    (i & 1) as f32 * 2.0,
                    ((i >> 1) & 1) as f32 * 2.0,
                    ((i >> 2) & 1) as f32 * 2.0,
                )
            })
            .collect();
        assert_eq!(lattice_points(&quickhull(&cube).unwrap()), 27);
        // An L of voxels: its hull holds one more.
        let mut rows = Rows::new([4, 4, 4]);
        for v in [
            [0, 0, 0],
            [1, 0, 0],
            [2, 0, 0],
            [0, 1, 0],
            [0, 2, 0],
            [0, 0, 1],
            [1, 0, 1],
            [2, 0, 1],
            [0, 1, 1],
            [0, 2, 1],
        ] {
            rows.add(v);
        }
        assert_eq!(rows.extra(), 2.0);
    }

    #[test]
    fn no_triangles_fail() {
        let e = decompose(&[], &[], &Settings::default()).unwrap_err();
        assert_eq!(e, "the model has no triangles to make a collider of");
        let e = decompose(&[[0.0; 3]], &[[0, 0, 1]], &Settings::default()).unwrap_err();
        assert_eq!(e, "a triangle uses a vertex the model does not have");
    }

    #[test]
    fn settings_out_of_range_fail() {
        assert_eq!(Settings::default().validate(), Ok(()));
        let (p, t) = l_shape();
        for (settings, error) in [
            (
                Settings {
                    max_hulls: 300,
                    ..Settings::default()
                },
                "max_hulls must be 1 to 256, not 300",
            ),
            (
                Settings {
                    max_vertices: 3,
                    ..Settings::default()
                },
                "max_vertices must be 4 to 255, not 3",
            ),
            (
                Settings {
                    error: 0.0,
                    ..Settings::default()
                },
                "error must be 0.001 to 0.5, not 0",
            ),
            (
                Settings {
                    resolution: 1000,
                    ..Settings::default()
                },
                "resolution must be 16 to 256, not 1000",
            ),
        ] {
            assert_eq!(settings.validate().unwrap_err(), error);
            assert_eq!(decompose(&p, &t, &settings).unwrap_err(), error);
        }
    }
}
