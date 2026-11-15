//! Color spaces and color notations: sRGB and linear light, HSV, HSL, luminance and hex.

use std::fmt;
use std::sync::LazyLock;

/// How a texture's color channels are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// sRGB transfer function on R, G and B; alpha is linear.
    Srgb,
    /// Values as stored.
    Linear,
}

impl fmt::Display for ColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Srgb => "srgb",
            Self::Linear => "linear",
        })
    }
}

/// An sRGB-encoded channel (0–1) as linear light.
pub fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// A linear-light channel (0–1) sRGB-encoded.
pub fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

static SRGB_TO_LINEAR: LazyLock<[f32; 256]> =
    LazyLock::new(|| std::array::from_fn(|i| srgb_to_linear(i as f32 / 255.0)));

/// An sRGB-encoded byte as linear light (0–1), through a table.
pub fn srgb_byte_to_linear(byte: u8) -> f32 {
    SRGB_TO_LINEAR[byte as usize]
}

/// A linear-light channel as an sRGB-encoded byte, clamped to 0–1 first.
pub fn linear_to_srgb_byte(v: f32) -> u8 {
    (linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8
}

/// Relative luminance of linear RGB, with the Rec. 709 (sRGB) primaries' weights.
pub fn luminance([r, g, b]: [f64; 3]) -> f64 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Hue, saturation and value, each 0–1, of an RGB color.
pub fn rgb_to_hsv([r, g, b]: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = [r, g, b].map(|c| f32::from(c) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    [hue, saturation, max]
}

/// The RGB color of hue, saturation and value, each 0–1 (hue wraps around).
pub fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [u8; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|channel| ((channel + m) * 255.0).round() as u8)
}

/// The RGB color (0–1, unclamped) of a hue in degrees (0–360), a saturation and a lightness
/// (0–1).
pub fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let h = (hue / 60.0).clamp(0.0, 6.0);
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = lightness - c / 2.0;
    [r + m, g + m, b + m]
}

/// `#rrggbb` or `#rrggbbaa` (the `#` optional, any case) as straight RGBA.
pub fn parse_hex(text: &str) -> Option<[u8; 4]> {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !matches!(hex.len(), 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok();
    let alpha = if hex.len() == 8 { byte(3)? } else { 255 };
    Some([byte(0)?, byte(1)?, byte(2)?, alpha])
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque.
pub fn format_hex([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_round_trips_every_byte() {
        for byte in 0..=255u8 {
            assert_eq!(linear_to_srgb_byte(srgb_byte_to_linear(byte)), byte);
            assert_eq!(
                srgb_byte_to_linear(byte),
                srgb_to_linear(byte as f32 / 255.0)
            );
        }
        assert_eq!(linear_to_srgb_byte(0.5), 188);
        assert_eq!(linear_to_srgb_byte(2.0), 255);
        assert_eq!(ColorSpace::Srgb.to_string(), "srgb");
    }

    #[test]
    fn hsv_round_trips() {
        for rgb in [
            [255, 0, 0],
            [0, 128, 255],
            [12, 200, 77],
            [255, 255, 255],
            [0, 0, 0],
            [128, 128, 128],
        ] {
            assert_eq!(hsv_to_rgb(rgb_to_hsv(rgb)), rgb, "{rgb:?}");
        }
        assert_eq!(rgb_to_hsv([0, 255, 0])[0], 1.0 / 3.0);
        assert_eq!(hsv_to_rgb([1.0, 1.0, 1.0]), [255, 0, 0]);
    }

    #[test]
    fn hsl_and_luminance() {
        assert_eq!(hsl_to_rgb(0.0, 1.0, 0.5), [1.0, 0.0, 0.0]);
        assert_eq!(hsl_to_rgb(120.0, 0.0, 0.25), [0.25, 0.25, 0.25]);
        assert!((luminance([1.0, 1.0, 1.0]) - 1.0).abs() < 1e-12);
        assert_eq!(luminance([0.0, 1.0, 0.0]), 0.7152);
    }

    #[test]
    fn hex_parses_and_formats() {
        assert_eq!(parse_hex("#FF8000"), Some([255, 128, 0, 255]));
        assert_eq!(parse_hex("ff800080"), Some([255, 128, 0, 128]));
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(format_hex([255, 128, 0, 255]), "#ff8000");
        assert_eq!(format_hex([255, 128, 0, 128]), "#ff800080");
    }
}
