//! Gradients as stops: blending and dithering, and the image of one; see
//! docs/spec/agradient.md#image in the assetc repository. Rendering follows `img/gradient.go`
//! of SoftKiwiGames/cli.

const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

const BAYER8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// How a gradient's blend is rounded to bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Dither {
    /// To the nearest byte.
    #[default]
    None,
    /// With an 8 × 8 Bayer threshold per channel.
    Crosshatch,
    /// Each pixel one of the two stops around it, by a 4 × 4 Bayer threshold.
    Pixel,
}

impl Dither {
    pub const ALL: [Self; 3] = [Self::None, Self::Crosshatch, Self::Pixel];

    /// Its key in files.
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Crosshatch => "crosshatch",
            Self::Pixel => "pixel",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Crosshatch => "Crosshatch",
            Self::Pixel => "Pixel",
        }
    }
}

/// A color at a position (0–1); `color` is straight (not premultiplied) RGBA.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stop {
    pub at: f64,
    pub color: [u8; 4],
}

/// The color of `stops` (at least two, ordered by `at`) at `t`, without dithering.
pub fn color_at(stops: &[Stop], t: f64) -> [u8; 4] {
    pixel(stops, Dither::None, t, 0, 0)
}

/// Position of pixel `i` along a gradient `length` pixels long: the ends are exact.
fn position(i: u32, length: u32) -> f64 {
    if length > 1 {
        i as f64 / (length - 1) as f64
    } else {
        0.0
    }
}

/// The color of pixel `(x, y)` (where the dither pattern starts at 0, 0) at position `t` of
/// `stops` (at least two, ordered by `at`): blended premultiplied, so fading to transparent
/// keeps the color.
pub fn pixel(stops: &[Stop], dither: Dither, t: f64, x: u32, y: u32) -> [u8; 4] {
    let mut k = 0;
    while k + 2 < stops.len() && t > stops[k + 1].at {
        k += 1;
    }
    let (a, b) = (stops[k], stops[k + 1]);
    let span = b.at - a.at;
    let s = if span > 0.0 {
        ((t - a.at) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (x, y) = (x as usize, y as usize);
    let d = match dither {
        Dither::Pixel => {
            let threshold = f64::from(BAYER4[y % 4][x % 4]) / 16.0;
            return if s > threshold { b.color } else { a.color };
        }
        Dither::None => 0.5,
        Dither::Crosshatch => f64::from(BAYER8[y % 8][x % 8]) / 64.0,
    };
    let (aa, ba) = (f64::from(a.color[3]), f64::from(b.color[3]));
    let alpha = aa + (ba - aa) * s;
    let stored = quantize(alpha, 0.5);
    if stored == 0 {
        return [0; 4];
    }
    let channel = |c: usize| {
        let pa = f64::from(a.color[c]) * aa / 255.0;
        let pb = f64::from(b.color[c]) * ba / 255.0;
        quantize((pa + (pb - pa) * s) * 255.0 / alpha, d)
    };
    [channel(0), channel(1), channel(2), stored]
}

fn quantize(v: f64, d: f64) -> u8 {
    (v + d).floor().clamp(0.0, 255.0) as u8
}

/// `width × height` RGBA8 of a gradient standing up, as a cell of a gradient image: the first
/// stop on the bottom row, the last on the top one.
pub fn render(stops: &[Stop], dither: Dither, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let t = position(height - 1 - y, height);
        for x in 0..width {
            pixels.extend(pixel(stops, dither, t, x, y));
        }
    }
    pixels
}

/// `width × height` RGBA8 of a gradient on its side, first stop on the left.
pub fn render_sideways(stops: &[Stop], dither: Dither, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend(pixel(stops, dither, position(x, width), x, y));
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black_to_white() -> Vec<Stop> {
        vec![
            Stop {
                at: 0.0,
                color: [0, 0, 0, 255],
            },
            Stop {
                at: 1.0,
                color: [255, 255, 255, 255],
            },
        ]
    }

    #[test]
    fn blending_is_premultiplied() {
        // Fading to transparent keeps the color instead of darkening toward black.
        let mut stops = black_to_white();
        stops[0].color = [255, 0, 0, 255];
        stops[1].color = [0, 0, 0, 0];
        assert_eq!(color_at(&stops, 0.5), [255, 0, 0, 128]);
        // Opaque black to white rounds to the middle.
        assert_eq!(color_at(&black_to_white(), 0.5), [128, 128, 128, 255]);
    }

    #[test]
    fn stops_in_between_pick_their_span() {
        let mut stops = black_to_white();
        stops.insert(
            1,
            Stop {
                at: 0.5,
                color: [255, 0, 0, 255],
            },
        );
        assert_eq!(color_at(&stops, 0.5), [255, 0, 0, 255]);
        assert_eq!(color_at(&stops, 0.75), [255, 128, 128, 255]);
        assert_eq!(color_at(&stops, 1.0), [255, 255, 255, 255]);
    }

    #[test]
    fn pixel_dither_keeps_the_palette() {
        let stops = black_to_white();
        for p in render(&stops, Dither::Pixel, 4, 64).as_chunks::<4>().0 {
            assert!(p == &[0, 0, 0, 255] || p == &[255, 255, 255, 255], "{p:?}");
        }
        // Crosshatch varies between neighbours where plain rounding does not.
        let column = render(&stops, Dither::Crosshatch, 1, 64);
        assert_ne!(column, render(&stops, Dither::None, 1, 64));
        // On its side, the same blend.
        assert_eq!(
            render_sideways(&stops, Dither::None, 64, 1)[..4],
            render(&stops, Dither::None, 1, 64)[63 * 4..]
        );
    }

    #[test]
    fn ends_are_exact() {
        let pixels = render(&black_to_white(), Dither::None, 1, 2);
        assert_eq!(pixels, [255, 255, 255, 255, 0, 0, 0, 255]);
        assert_eq!(
            render(&black_to_white(), Dither::None, 1, 1),
            [0, 0, 0, 255]
        );
    }
}
