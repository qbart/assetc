//! Authored color sets: each vertex's color from its position; see
//! docs/features/vertex-colors.md#authored-sets.

use bevy::math::Vec3;

use crate::enrich::{ColorFill, ColorSet};
use crate::gradient;

/// An sRGB channel as a linear one, both 8 bits.
fn linear(c: u8) -> u8 {
    let c = f32::from(c) / 255.0;
    let l = if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    };
    (l * 255.0).round() as u8
}

/// sRGB RGBA as stored in a set: R, G and B linear, alpha as it is.
pub fn stored([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    [linear(r), linear(g), linear(b), a]
}

/// Position along the gradient of a vertex at `p`, 0 to 1.
fn position(set: &ColorSet, p: Vec3) -> f64 {
    let from = Vec3::from(set.from);
    let span = Vec3::from(set.to) - from;
    let length2 = span.length_squared();
    if length2 <= f32::EPSILON * f32::EPSILON {
        return 0.0;
    }
    let t = match set.fill {
        ColorFill::Solid => 0.0,
        ColorFill::Linear => (p - from).dot(span) / length2,
        ColorFill::Radial => (p - from).length() / length2.sqrt(),
    };
    f64::from(t.clamp(0.0, 1.0))
}

/// The stored (linear) color of a vertex at `p`.
pub fn evaluate(set: &ColorSet, p: Vec3) -> [u8; 4] {
    stored(match set.fill {
        ColorFill::Solid => set.color,
        ColorFill::Linear | ColorFill::Radial => gradient::color_at(&set.stops, position(set, p)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradients_follow_the_line() {
        let mut set = ColorSet::new("g".into(), [0.0, 0.0, 0.0], [0.0, 2.0, 0.0]);
        // Below the start, at the end and past it.
        assert_eq!(evaluate(&set, Vec3::new(5.0, -1.0, 0.0)), [0, 0, 0, 255]);
        assert_eq!(evaluate(&set, Vec3::new(0.0, 2.0, 3.0)), [255; 4]);
        assert_eq!(evaluate(&set, Vec3::new(0.0, 9.0, 0.0)), [255; 4]);
        // Halfway: sRGB 128 (rounded from 127.5), stored linear.
        let middle = evaluate(&set, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(middle, [55, 55, 55, 255]);
        set.fill = ColorFill::Radial;
        assert_eq!(evaluate(&set, Vec3::new(2.0, 0.0, 0.0)), [255; 4]);
        assert_eq!(evaluate(&set, Vec3::new(0.0, -1.0, 0.0)), middle);
        // Ends at one point: the first stop.
        set.to = set.from;
        assert_eq!(evaluate(&set, Vec3::ONE), [0, 0, 0, 255]);
        set.fill = ColorFill::Solid;
        set.color = [255, 128, 0, 7];
        assert_eq!(evaluate(&set, Vec3::ONE), [255, 55, 0, 7]);
    }
}
