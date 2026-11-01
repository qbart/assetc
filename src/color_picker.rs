//! Inline color picker: saturation / value square, hue and alpha bars, hex and RGBA fields.
//! See docs/features/gradients.md#selected-stop.

use bevy_egui::egui::{
    self, Color32, ColorImage, Pos2, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui, pos2,
    vec2,
};

use crate::gradient::{format_color, parse_color};
use crate::theme::{BG_BAR, CHECKER_DARK, CHECKER_LIGHT, TEXT_WEAK};

/// A picker's state between frames.
#[derive(Default)]
pub struct Picker {
    /// The HSV last picked and the color it made: keeps hue and saturation where the color
    /// alone cannot tell them (grays, black).
    hsv: Option<([u8; 4], [f32; 3])>,
    /// The hex field's text while it is being edited.
    hex: Option<String>,
    /// Saturation / value square for a hue.
    square: Option<(f32, TextureHandle)>,
    hue: Option<TextureHandle>,
    /// Alpha bar for a color.
    alpha: Option<([u8; 3], TextureHandle)>,
}

impl Picker {
    /// Shows the picker for `color` (straight RGBA) across the available width; returns
    /// whether it changed.
    pub fn show(&mut self, ui: &mut Ui, color: &mut [u8; 4]) -> bool {
        let width = ui.available_width();
        let mut hsv = match self.hsv {
            Some((made, hsv)) if made == *color => hsv,
            _ => rgb_to_hsv([color[0], color[1], color[2]]),
        };
        let before = *color;
        let ctx = ui.ctx().clone();
        let linear = TextureOptions::LINEAR;

        // Saturation left to right, value top to bottom.
        let (rect, response) = ui.allocate_exact_size(vec2(width, 120.0), Sense::click_and_drag());
        if self.square.as_ref().is_none_or(|(hue, _)| *hue != hsv[0]) {
            const N: usize = 64;
            let mut pixels = Vec::with_capacity(N * N * 4);
            for y in 0..N {
                for x in 0..N {
                    let s = x as f32 / (N - 1) as f32;
                    let v = 1.0 - y as f32 / (N - 1) as f32;
                    pixels.extend(hsv_to_rgb([hsv[0], s, v]));
                    pixels.push(255);
                }
            }
            let image = ColorImage::from_rgba_unmultiplied([N, N], &pixels);
            self.square = Some((hsv[0], ctx.load_texture("picker_square", image, linear)));
        }
        if let Some((_, texture)) = &self.square {
            paint(ui, texture, rect);
        }
        if let Some(p) = pressed(&response) {
            hsv[1] = fraction(p.x, rect.left(), rect.width());
            hsv[2] = 1.0 - fraction(p.y, rect.top(), rect.height());
            set_rgb(color, hsv);
        }
        let ring = pos2(
            rect.left() + hsv[1] * rect.width(),
            rect.top() + (1.0 - hsv[2]) * rect.height(),
        );
        ui.painter()
            .circle_stroke(ring, 7.5, Stroke::new(1.0, BG_BAR));
        ui.painter()
            .circle_stroke(ring, 6.0, Stroke::new(2.0, Color32::WHITE));
        ui.add_space(6.0);

        // Hue circle left to right.
        let (rect, response) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
        let hue = self.hue.get_or_insert_with(|| {
            let pixels: Vec<u8> = (0..360)
                .flat_map(|h| {
                    let [r, g, b] = hsv_to_rgb([h as f32 / 359.0, 1.0, 1.0]);
                    [r, g, b, 255]
                })
                .collect();
            let image = ColorImage::from_rgba_unmultiplied([360, 1], &pixels);
            ctx.load_texture("picker_hue", image, linear)
        });
        paint(ui, hue, rect);
        if let Some(p) = pressed(&response) {
            hsv[0] = fraction(p.x, rect.left(), rect.width());
            set_rgb(color, hsv);
        }
        bar_marker(ui, rect, hsv[0]);
        ui.add_space(6.0);

        // Alpha, transparent to opaque, over a checkerboard.
        let (rect, response) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
        let rgb = [color[0], color[1], color[2]];
        if self.alpha.as_ref().is_none_or(|(made, _)| *made != rgb) {
            let pixels: Vec<u8> = (0..=255)
                .flat_map(|a| [rgb[0], rgb[1], rgb[2], a])
                .collect();
            let image = ColorImage::from_rgba_unmultiplied([256, 1], &pixels);
            self.alpha = Some((rgb, ctx.load_texture("picker_alpha", image, linear)));
        }
        checker(ui, rect, 4);
        if let Some((_, texture)) = &self.alpha {
            paint(ui, texture, rect);
        }
        if let Some(p) = pressed(&response) {
            color[3] = (fraction(p.x, rect.left(), rect.width()) * 255.0).round() as u8;
        }
        bar_marker(ui, rect, f32::from(color[3]) / 255.0);
        ui.add_space(8.0);

        // Swatch and hex.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (swatch, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
            checker(ui, swatch, 4);
            ui.painter().rect_filled(swatch, 4, to_color32(*color));
            let mut text = self.hex.take().unwrap_or_else(|| format_color(*color));
            let field = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(ui.available_width())
                    .font(egui::TextStyle::Monospace),
            );
            if field.has_focus() {
                self.hex = Some(text);
            } else if field.lost_focus()
                && let Some(parsed) = parse_color(&text)
            {
                *color = parsed;
            }
        });

        // R, G, B, A fields.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let each = (ui.available_width() - 3.0 * 8.0) / 4.0;
            for (i, letter) in ["R", "G", "B", "A"].into_iter().enumerate() {
                if i > 0 {
                    ui.add_space(4.0);
                }
                ui.add_sized(
                    vec2(10.0, 22.0),
                    egui::Label::new(egui::RichText::new(letter).color(TEXT_WEAK)),
                );
                ui.add_sized(
                    vec2((each - 14.0).max(24.0), 22.0),
                    egui::DragValue::new(&mut color[i]).range(0..=255),
                );
            }
        });

        self.hsv = Some((*color, hsv));
        *color != before
    }
}

/// The pointer position while `response` is pressed or dragged.
fn pressed(response: &egui::Response) -> Option<Pos2> {
    response
        .is_pointer_button_down_on()
        .then(|| response.interact_pointer_pos())
        .flatten()
}

fn fraction(v: f32, start: f32, length: f32) -> f32 {
    ((v - start) / length).clamp(0.0, 1.0)
}

fn set_rgb(color: &mut [u8; 4], hsv: [f32; 3]) {
    let [r, g, b] = hsv_to_rgb(hsv);
    *color = [r, g, b, color[3]];
}

fn paint(ui: &Ui, texture: &TextureHandle, rect: Rect) {
    egui::Image::from_texture((texture.id(), rect.size()))
        .corner_radius(4)
        .paint_at(ui, rect);
}

/// A vertical marker at `fraction` across a bar.
fn bar_marker(ui: &Ui, rect: Rect, fraction: f32) {
    let x = rect.left() + fraction * rect.width();
    let marker = Rect::from_center_size(pos2(x, rect.center().y), vec2(4.0, rect.height() + 4.0));
    ui.painter().rect(
        marker,
        2,
        Color32::WHITE,
        Stroke::new(1.0, BG_BAR),
        egui::StrokeKind::Outside,
    );
}

pub fn to_color32([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// An 8 px `CHECKER_LIGHT` / `CHECKER_DARK` checkerboard filling `rect`.
pub fn checker(ui: &Ui, rect: Rect, radius: u8) {
    let ctx = ui.ctx();
    let id = egui::Id::new("picker_checker");
    let texture = ctx
        .data(|d| d.get_temp::<TextureHandle>(id))
        .unwrap_or_else(|| {
            let [l, d] = [CHECKER_LIGHT, CHECKER_DARK].map(|c| [c.r(), c.g(), c.b(), 255]);
            let pixels = [l, d, d, l].concat();
            let image = ColorImage::from_rgba_unmultiplied([2, 2], &pixels);
            let options = TextureOptions {
                wrap_mode: egui::TextureWrapMode::Repeat,
                ..TextureOptions::NEAREST
            };
            let texture = ctx.load_texture("checker", image, options);
            ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
            texture
        });
    // Two 8 px squares per texture repeat.
    let uv = Rect::from_min_size(Pos2::ZERO, rect.size() / 16.0);
    egui::Image::from_texture((texture.id(), rect.size()))
        .uv(uv)
        .corner_radius(radius)
        .paint_at(ui, rect);
}

/// Hue, saturation, value in 0–1.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
