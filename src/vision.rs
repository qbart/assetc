//! Color vision deficiency simulation for the canvas; see docs/features/canvas.md#color-vision.

use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vision {
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Monochromacy,
}

impl Vision {
    pub const ALL: [Self; 4] = [
        Self::Protanopia,
        Self::Deuteranopia,
        Self::Tritanopia,
        Self::Monochromacy,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Protanopia => "Protanopia",
            Self::Deuteranopia => "Deuteranopia",
            Self::Tritanopia => "Tritanopia",
            Self::Monochromacy => "Monochromacy",
        }
    }

    /// What it is and how common, for the button's tooltip; see
    /// docs/features/canvas.md#color-vision.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Protanopia => {
                "Red-Green Color Blindness (protan): no working red (L) cones, so reds look \
darker and are confused with greens. Protanopia affects roughly 1% of males; all \
red-green forms together (including the milder protanomaly and deuteranomaly) \
affect about 8% of males and 0.5% of females of Northern European descent."
            }
            Self::Deuteranopia => {
                "Red-Green Color Blindness (deutan): no working green (M) cones, so reds, oranges \
and greens are confused. Deuteranopia affects roughly 1% of males; its milder \
form, deuteranomaly, is the most common color vision deficiency (about 5% of \
males). All red-green forms together affect about 8% of males and 0.5% of \
females of Northern European descent."
            }
            Self::Tritanopia => {
                "Blue-Yellow Color Blindness (tritan): no working blue (S) cones, so blue and \
green, and yellow and pink, are confused. Very rare when inherited (estimates \
range from about 1 in 10,000 to 1 in 20,000 people, roughly 0.005% to 0.01%) and \
affects men and women equally, as it is not X-linked. Acquired tritan defects \
(ageing, eye disease) are more common."
            }
            Self::Monochromacy => {
                "Complete Color Blindness (achromatopsia, rod monochromacy): no working cones, so \
everything is seen in shades of gray, usually with light sensitivity and low \
sharpness. About 1 in 30,000 people; blue cone monochromacy is rarer, about 1 in \
100,000."
            }
        }
    }

    /// Rows of the matrix applied to linear RGB: Machado, Oliveira and Fernandes (2009) at
    /// full severity, or Rec. 709 luminance.
    pub fn matrix(self) -> [[f32; 3]; 3] {
        match self {
            Self::Protanopia => [
                [0.152286, 1.052583, -0.204868],
                [0.114503, 0.786281, 0.099216],
                [-0.003882, -0.048116, 1.051998],
            ],
            Self::Deuteranopia => [
                [0.367322, 0.860646, -0.227968],
                [0.280085, 0.672501, 0.047413],
                [-0.011820, 0.042940, 0.968881],
            ],
            Self::Tritanopia => [
                [1.255528, -0.076749, -0.178779],
                [-0.078411, 0.930809, 0.147602],
                [0.004733, 0.691367, 0.303900],
            ],
            Self::Monochromacy => [[0.2126, 0.7152, 0.0722]; 3],
        }
    }

    /// RGBA8 pixels (sRGB encoded) as this deficiency sees them; alpha is kept.
    pub fn simulate(self, pixels: &[u8]) -> Vec<u8> {
        static LINEAR: LazyLock<[f32; 256]> =
            LazyLock::new(|| std::array::from_fn(|v| srgb_to_linear(v as f32 / 255.0)));
        let m = self.matrix();
        let mut out = Vec::with_capacity(pixels.len());
        for px in pixels.as_chunks::<4>().0 {
            let rgb = [
                LINEAR[px[0] as usize],
                LINEAR[px[1] as usize],
                LINEAR[px[2] as usize],
            ];
            for row in m {
                let v = (row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]).clamp(0.0, 1.0);
                out.push((crate::texture::linear_to_srgb(v) * 255.0).round() as u8);
            }
            out.push(px[3]);
        }
        out
    }
}

fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_match_the_spec() {
        let spec = include_str!("../docs/features/canvas.md");
        for vision in Vision::ALL {
            let line = format!("- **{}**: {}", vision.name(), vision.hint());
            assert!(spec.contains(&line), "{line}");
        }
    }

    #[test]
    fn grays_and_alpha_stay() {
        for vision in Vision::ALL {
            for row in vision.matrix() {
                assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-3, "{vision:?}");
            }
            let gray = [0, 0, 0, 10, 128, 128, 128, 200, 255, 255, 255, 255];
            assert_eq!(vision.simulate(&gray), gray, "{vision:?}");
        }
    }

    #[test]
    fn colors_shift() {
        // Pure red: dark yellow-brown to a protanope, a gray in monochromacy.
        let red = [255, 0, 0, 255];
        let p = Vision::Protanopia.simulate(&red);
        assert!(p[0] < 120 && p[1] > p[2], "{p:?}");
        let m = Vision::Monochromacy.simulate(&red);
        assert!(m[0] == m[1] && m[1] == m[2] && m[0] > 100, "{m:?}");
        // Red and green, told apart by color, look alike to a deuteranope.
        let d = Vision::Deuteranopia.simulate(&[255, 0, 0, 255, 0, 160, 0, 255]);
        let diff: i32 = (0..3).map(|c| (d[c] as i32 - d[4 + c] as i32).abs()).sum();
        assert!(diff < 120, "{d:?}");
    }
}
