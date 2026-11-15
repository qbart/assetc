//! Blend modes: how a color is laid over another; see docs/features/vertex-colors.md#blending
//! in the assetc repository.

/// How a color is laid over the one under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// The color on top.
    #[default]
    Mix,
    Multiply,
    Screen,
    /// Multiply where the color under it is dark, screen where it is light.
    Overlay,
    /// The sum, at most 1.
    Add,
}

impl BlendMode {
    pub const ALL: [Self; 5] = [
        Self::Mix,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Add,
    ];

    /// Its key in files.
    pub fn key(self) -> &'static str {
        match self {
            Self::Mix => "mix",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Add => "add",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Mix => "Mix",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Overlay => "Overlay",
            Self::Add => "Add",
        }
    }

    /// One channel (0–1) of `top` laid over `base` in this mode, at full strength.
    pub fn channel(self, base: f32, top: f32) -> f32 {
        match self {
            Self::Mix => top,
            Self::Multiply => base * top,
            Self::Screen => 1.0 - (1.0 - base) * (1.0 - top),
            Self::Overlay if base < 0.5 => 2.0 * base * top,
            Self::Overlay => 1.0 - 2.0 * (1.0 - base) * (1.0 - top),
            Self::Add => (base + top).min(1.0),
        }
    }
}

/// `top` laid over `base` (RGBA, 0–1, straight alpha) in `mode`, as far as `amount` (0–1) and
/// `top`'s alpha take it; the alpha covers what `base`'s did not, by as much.
pub fn blend(mode: BlendMode, base: [f32; 4], top: [f32; 4], amount: f32) -> [f32; 4] {
    let k = top[3] * amount;
    let mut out = [0.0; 4];
    for c in 0..3 {
        out[c] = base[c] + (mode.channel(base[c], top[c]) - base[c]) * k;
    }
    out[3] = base[3] + (1.0 - base[3]) * k;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_lay_colors_over() {
        let (base, top) = ([0.25, 0.5, 0.75, 1.0], [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(blend(BlendMode::Mix, base, top, 1.0), [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(
            blend(BlendMode::Multiply, base, top, 1.0),
            [0.125, 0.25, 0.375, 1.0]
        );
        assert_eq!(
            blend(BlendMode::Screen, base, top, 1.0),
            [0.625, 0.75, 0.875, 1.0]
        );
        assert_eq!(
            blend(BlendMode::Overlay, base, top, 1.0),
            [0.25, 0.5, 0.75, 1.0]
        );
        assert_eq!(blend(BlendMode::Add, base, top, 1.0), [0.75, 1.0, 1.0, 1.0]);
        // Half the amount, or a half transparent top: halfway.
        assert_eq!(blend(BlendMode::Mix, base, top, 0.5)[0], 0.375);
        let faint = [0.5, 0.5, 0.5, 0.5];
        assert_eq!(
            blend(BlendMode::Mix, [0.0, 0.0, 0.0, 0.0], faint, 1.0),
            [0.25, 0.25, 0.25, 0.5]
        );
    }
}
