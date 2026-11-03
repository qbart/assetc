//! Path curves: cardinal splines through control points, open or closed, measured and
//! sampled by distance; see docs/features/paths.md#curve.

use matma::{Spline, cardinal, cardinal_tangent, float3};

/// Steps of the spline parameter per segment when measuring a curve.
const MEASURE_STEPS: usize = 32;

/// Samples per segment written to the `.amesh.bin`; see docs/spec/amesh.bin.md#paths.
pub const SAMPLES_PER_SEGMENT: usize = 16;

/// A path's curve.
pub struct Curve {
    /// Each segment's four knots: before, start, end, after.
    segments: Vec<[float3; 4]>,
    tension: f32,
    alpha: f32,
}

/// A point of a curve sampled by distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub position: [f32; 3],
    /// From the start, along the curve.
    pub distance: f32,
    /// Unit direction of travel; zero where the curve stops.
    pub tangent: [f32; 3],
    /// The spline parameter: segment index plus the position in it.
    pub u: f32,
}

/// Fewest points a curve needs.
pub fn min_points(closed: bool) -> usize {
    if closed { 3 } else { 2 }
}

impl Curve {
    /// The curve through `points`; `None` with too few points.
    pub fn new(points: &[[f32; 3]], closed: bool, tension: f32, alpha: f32) -> Option<Self> {
        let n = points.len();
        if n < min_points(closed) {
            return None;
        }
        let points: Vec<float3> = points.iter().map(|p| float3::from_array(*p)).collect();
        let segments = if closed {
            let at = |i: isize| points[i.rem_euclid(n as isize) as usize];
            (0..n as isize)
                .map(|s| [at(s - 1), at(s), at(s + 1), at(s + 2)])
                .collect()
        } else {
            // matma's knots: phantom ones mirrored past either end.
            let spline = Spline::from_points(points);
            (0..spline.segment_count() as isize)
                .map(|s| {
                    [
                        spline.knot(s - 1),
                        spline.knot(s),
                        spline.knot(s + 1),
                        spline.knot(s + 2),
                    ]
                })
                .collect()
        };
        Some(Self {
            segments,
            tension,
            alpha,
        })
    }

    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Segment and its `t` for the spline parameter `u`, clamped to the curve.
    fn locate(&self, u: f32) -> ([float3; 4], f32) {
        let last = self.segments.len() - 1;
        let u = u.clamp(0.0, self.segments.len() as f32);
        let s = (u.floor() as usize).min(last);
        (self.segments[s], u - s as f32)
    }

    /// The point at spline parameter `u`.
    pub fn evaluate(&self, u: f32) -> [f32; 3] {
        let ([p0, p1, p2, p3], t) = self.locate(u);
        cardinal(p0, p1, p2, p3, t, self.tension, self.alpha).to_array()
    }

    /// The velocity at spline parameter `u`.
    pub fn velocity(&self, u: f32) -> [f32; 3] {
        let ([p0, p1, p2, p3], t) = self.locate(u);
        cardinal_tangent(p0, p1, p2, p3, t, self.tension, self.alpha).to_array()
    }

    /// `steps` points per segment plus the end, evenly spaced in `u`.
    pub fn polyline(&self, steps: usize) -> Vec<[f32; 3]> {
        let total = self.segments.len() * steps;
        (0..=total)
            .map(|i| self.evaluate(i as f32 / steps as f32))
            .collect()
    }

    /// The cumulative length at each of `MEASURE_STEPS` steps per segment: `(u, length)`.
    fn table(&self) -> Vec<(f32, f32)> {
        let total = self.segments.len() * MEASURE_STEPS;
        let mut table = Vec::with_capacity(total + 1);
        let mut previous = float3::from_array(self.evaluate(0.0));
        let mut length = 0.0;
        table.push((0.0, 0.0));
        for i in 1..=total {
            let u = i as f32 / MEASURE_STEPS as f32;
            let p = float3::from_array(self.evaluate(u));
            length += (p - previous).length();
            table.push((u, length));
            previous = p;
        }
        table
    }

    /// The curve's length.
    pub fn length(&self) -> f32 {
        self.table().last().map_or(0.0, |e| e.1)
    }

    /// `SAMPLES_PER_SEGMENT` samples per segment plus one, evenly spaced by distance.
    pub fn samples(&self) -> Vec<Sample> {
        let table = self.table();
        let length = table.last().map_or(0.0, |e| e.1);
        let count = self.segments.len() * SAMPLES_PER_SEGMENT + 1;
        (0..count)
            .map(|k| {
                let distance = length * k as f32 / (count - 1) as f32;
                // The table entry reaching `distance`, then `u` in proportion within it.
                let i = table.partition_point(|e| e.1 < distance).max(1);
                let u = match table.get(i) {
                    Some(&(ub, lb)) => {
                        let (ua, la) = table[i - 1];
                        let span = lb - la;
                        if span > 1e-9 {
                            ua + (ub - ua) * (distance - la) / span
                        } else {
                            ua
                        }
                    }
                    None => self.segments.len() as f32,
                };
                let u = if k == 0 {
                    0.0
                } else if k == count - 1 {
                    self.segments.len() as f32
                } else {
                    u
                };
                let velocity = float3::from_array(self.velocity(u));
                let speed = velocity.length();
                let tangent = if speed > 1e-9 {
                    (velocity / speed).to_array()
                } else {
                    [0.0; 3]
                };
                Sample {
                    position: self.evaluate(u),
                    distance,
                    tangent,
                    u,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn straight_lines_measure_exactly() {
        let points = [[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 4.0, 0.0]];
        let curve = Curve::new(&points, false, 1.0, 0.5).unwrap();
        assert_eq!(curve.segment_count(), 2);
        assert!((curve.length() - 7.0).abs() < 1e-4);
        let samples = curve.samples();
        assert_eq!(samples.len(), 2 * SAMPLES_PER_SEGMENT + 1);
        assert!(close(samples[0].position, points[0]));
        assert!(close(samples.last().unwrap().position, points[2]));
        // Evenly spaced by distance: the middle one is 3.5 along, on the second leg.
        let middle = samples[SAMPLES_PER_SEGMENT];
        assert!((middle.distance - 3.5).abs() < 1e-4);
        let [x, y, _] = middle.position;
        assert!(
            (x - 3.0).abs() < 1e-4 && (y - 0.5).abs() < 1e-3,
            "{middle:?}"
        );
        assert!(close(middle.tangent, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn curves_pass_through_their_points() {
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [2.0, 0.0, 1.0],
            [3.0, 1.0, 1.0],
        ];
        let curve = Curve::new(&points, false, 0.0, 0.5).unwrap();
        for (i, p) in points.iter().enumerate() {
            assert!(close(curve.evaluate(i as f32), *p));
        }
        // Longer than the straight lines through the points.
        let chords: f32 = points
            .windows(2)
            .map(|w| (float3::from_array(w[1]) - float3::from_array(w[0])).length())
            .sum();
        assert!(curve.length() > chords);
    }

    #[test]
    fn closed_curves_loop() {
        let square = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let curve = Curve::new(&square, true, 0.0, 0.5).unwrap();
        assert_eq!(curve.segment_count(), 4);
        let samples = curve.samples();
        assert!(close(samples.last().unwrap().position, square[0]));
        // Smooth through the first point: the same direction leaving and arriving.
        assert!(close(samples[0].tangent, samples.last().unwrap().tangent));
        assert!(Curve::new(&square[..2], true, 0.0, 0.5).is_none());
        assert!(Curve::new(&square[..1], false, 0.0, 0.5).is_none());
    }
}
