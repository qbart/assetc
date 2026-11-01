//! The built-in reference chart for LUT previews, see docs/features/lut.md#reference-chart.

use std::sync::LazyLock;

use crate::texture::Image;

pub const WIDTH: u32 = 768;
pub const HEIGHT: u32 = 512;
const SWEEP_HEIGHT: u32 = 256;
const RAMP_HEIGHT: u32 = 48;

/// X-Rite ColorChecker Classic, sRGB, row by row.
const COLOR_CHECKER: [[u8; 3]; 24] = [
    [115, 82, 68],   // dark skin
    [194, 150, 130], // light skin
    [98, 122, 157],  // blue sky
    [87, 108, 67],   // foliage
    [133, 128, 177], // blue flower
    [103, 189, 170], // bluish green
    [214, 126, 44],  // orange
    [80, 91, 166],   // purplish blue
    [193, 90, 99],   // moderate red
    [94, 60, 108],   // purple
    [157, 188, 64],  // yellow green
    [224, 163, 46],  // orange yellow
    [56, 61, 150],   // blue
    [70, 148, 73],   // green
    [175, 54, 60],   // red
    [231, 199, 31],  // yellow
    [187, 86, 149],  // magenta
    [8, 133, 161],   // cyan
    [243, 243, 242], // white
    [200, 200, 200], // neutral 8
    [160, 160, 160], // neutral 6.5
    [122, 122, 121], // neutral 5
    [85, 85, 85],    // neutral 3.5
    [52, 52, 52],    // black
];

pub static CHART: LazyLock<Image> = LazyLock::new(generate);

fn generate() -> Image {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let rgb = if y < SWEEP_HEIGHT {
                let hue = x as f32 / WIDTH as f32 * 360.0;
                let lightness = 1.0 - y as f32 / (SWEEP_HEIGHT - 1) as f32;
                hsl(hue, lightness).map(|v| (v * 255.0).round() as u8)
            } else if y < SWEEP_HEIGHT + RAMP_HEIGHT {
                let v = (x * 255 / (WIDTH - 1)) as u8;
                [v, v, v]
            } else {
                let (column, row) = (x / (WIDTH / 6), (y - SWEEP_HEIGHT - RAMP_HEIGHT) / 52);
                COLOR_CHECKER[(row * 6 + column) as usize]
            };
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Image {
        width: WIDTH,
        height: HEIGHT,
        pixels,
    }
}

/// HSL with saturation 1 to RGB in `0..1`.
fn hsl(hue: f32, lightness: f32) -> [f32; 3] {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()).max(0.0);
    let h = hue / 60.0;
    let x = chroma * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    [r + m, g + m, b + m].map(|v| v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(x: u32, y: u32) -> [u8; 3] {
        let i = ((y * WIDTH + x) * 4) as usize;
        CHART.pixels[i..i + 3].try_into().unwrap()
    }

    #[test]
    fn layout() {
        assert_eq!(CHART.pixels.len(), (WIDTH * HEIGHT * 4) as usize);
        // Sweep: white on top, black at the bottom, pure red in the middle at hue 0.
        assert_eq!(pixel(0, 0), [255, 255, 255]);
        assert_eq!(pixel(0, SWEEP_HEIGHT - 1), [0, 0, 0]);
        let near = |a: [u8; 3], b: [u8; 3]| (0..3).all(|c| a[c].abs_diff(b[c]) <= 2);
        assert!(near(pixel(0, 128), [255, 0, 0]), "{:?}", pixel(0, 128));
        assert!(near(pixel(256, 128), [0, 255, 0]), "{:?}", pixel(256, 128));
        // Gray ramp ends.
        assert_eq!(pixel(0, SWEEP_HEIGHT + 10), [0, 0, 0]);
        assert_eq!(pixel(WIDTH - 1, SWEEP_HEIGHT + 10), [255, 255, 255]);
        // ColorChecker: first and last patch.
        assert_eq!(pixel(10, HEIGHT - 200), [115, 82, 68]);
        assert_eq!(pixel(WIDTH - 1, HEIGHT - 1), [52, 52, 52]);
    }
}
