//! A heatmap: a weight as a color, as assetc shows texture paint weights; see
//! docs/features/texture-paint.md#heatmap in the assetc repository.

/// The sRGB color of weight `v` (0–255): gray at exactly 0, then blue, cyan, green, yellow and
/// red at 255, blended linearly between them.
pub fn heat(v: u8) -> [u8; 3] {
    const STOPS: [[f32; 3]; 5] = [
        [0.0, 0.0, 255.0],
        [0.0, 255.0, 255.0],
        [0.0, 255.0, 0.0],
        [255.0, 255.0, 0.0],
        [255.0, 0.0, 0.0],
    ];
    if v == 0 {
        return [128; 3];
    }
    // Blue at 1/255, red at 1.
    let t = f32::from(v - 1) / 254.0 * 4.0;
    let i = (t as usize).min(3);
    let f = t - i as f32;
    std::array::from_fn(|c| (STOPS[i][c] + (STOPS[i + 1][c] - STOPS[i][c]) * f).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_runs_from_blue_to_red_over_gray() {
        assert_eq!(heat(0), [128, 128, 128]);
        assert_eq!(heat(1), [0, 0, 255]);
        assert_eq!(heat(128), [0, 255, 0]);
        assert_eq!(heat(255), [255, 0, 0]);
        let [r, g, b] = heat(64);
        assert!(r == 0 && g > 250 && b == 255, "{r} {g} {b}");
    }
}
