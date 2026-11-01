//! Color vision deficiency simulation for the canvas; see docs/features/canvas.md#color-vision.

use std::sync::LazyLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vision {
    Protanopia,
    Deuteranopia,
    Tritanopia,
    Monochromacy,
}

const RED_GREEN: &str = "Red-Green Color Blindness: The most common form (including deuteranomaly \
and protanomaly), affecting about 8% of males and 0.5% of females of Northern European descent.";

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

    /// Who it affects, for the button's tooltip.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Protanopia | Self::Deuteranopia => RED_GREEN,
            Self::Tritanopia => {
                "Blue-Yellow Color Blindness (Tritan-type): Much rarer (affecting roughly 0.01% \
to 0.03% of the population) and affects men and women equally."
            }
            Self::Monochromacy => {
                "Complete Color Blindness (Achromatopsia): Extremely rare, affecting a tiny \
fraction of the global population."
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
        for px in pixels.chunks_exact(4) {
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
