//! The curve editor popover: presets, a square plot with split tangents and a playhead; see
//! docs/features/curves.md.

use serde_json::Value;

use super::*;
use crate::theme::CHANNEL_R;

/// Points per unit, on both axes.
const UNIT: f32 = 200.0;
/// The frame's `x` reach past 0..1, its `y` reach to start with, and how far it may grow.
const X_PAD: f32 = 0.1;
const Y_PAD: f32 = 0.25;
const Y_LIMIT: (f32, f32) = (-1.0, 2.0);
/// The `y` labels' column, the `x` labels' row, the gap to the lane and the lane.
const LABELS_W: f32 = 28.0;
const LABELS_H: f32 = 16.0;
const GAP: f32 = 8.0;
const LANE: f32 = 12.0;
/// How near the pointer must be to a point or handle end, in points.
const REACH: f32 = 7.0;
const HANDLE: f32 = 40.0;
const HANDLE_ACTIVE: f32 = 56.0;
/// The steepest slope's angle.
const MAX_ANGLE: f32 = 89.0;
/// One playhead pass, in seconds.
const PASS: f64 = 2.0;
pub(super) const WIDTH: f32 = 320.0;

/// Steps, playback and rotation mode, for the session; never in the data.
pub(super) struct CurveSettings {
    x_step: f32,
    y_step: f32,
    angle_step: f32,
    playing: bool,
    rotate: bool,
}

impl Default for CurveSettings {
    fn default() -> Self {
        Self {
            x_step: 0.1,
            y_step: 0.1,
            angle_step: 5.0,
            playing: true,
            rotate: false,
        }
    }
}

/// One curve's editor state.
#[derive(Default)]
pub(super) struct CurveState {
    point: Option<usize>,
    drag: Option<CurveDrag>,
    /// The chosen point is marked split while its slopes are still equal.
    split: bool,
    /// The active slope of a split point is `in`, else `out`.
    active_in: bool,
    /// The playhead's `x`, and when it last moved.
    phase: f32,
    last: Option<f64>,
    /// The frame's `y` range, kept while something is dragged.
    frame: Option<(f32, f32)>,
    /// The editor's area last frame, where its keys act.
    area: Option<Rect>,
}

#[derive(Clone, Copy)]
enum CurveDrag {
    Point(usize),
    /// A tangent handle: out (true) or in.
    Handle(usize, bool),
}

/// The presets: name and points `[x, y, in, out]`.
const PRESETS: [(&str, &[[f32; 4]]); 8] = [
    ("Linear", &[[0.0, 0.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]]),
    ("Ease in", &[[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 2.0, 2.0]]),
    ("Ease out", &[[0.0, 0.0, 2.0, 2.0], [1.0, 1.0, 0.0, 0.0]]),
    ("Ease in-out", &[[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0]]),
    ("Constant", &[[0.0, 1.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.0]]),
    (
        "Overshoot",
        &[[0.0, 0.0, 4.702, 4.702], [1.0, 1.0, 0.0, 0.0]],
    ),
    (
        "Bounce",
        &[
            [0.0, 0.0, 0.0, 0.0],
            [0.364, 1.0, 5.5, -2.75],
            [0.727, 1.0, 2.75, -1.375],
            [0.909, 1.0, 1.375, -0.688],
            [1.0, 1.0, 0.688, 0.688],
        ],
    ),
    (
        "Step",
        &[
            [0.0, 0.0, 0.0, 0.0],
            [0.5, 0.0, 0.0, 0.0],
            [0.5, 1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0, 0.0],
        ],
    ),
];

/// Whether a point's slopes are equal to three decimals.
fn joined(p: &[f32; 4]) -> bool {
    (p[2] * 1000.0).round() == (p[3] * 1000.0).round()
}

/// `slope` turned by `degrees` (counter-clockwise), within ±89°.
fn rotate(slope: f32, degrees: f32) -> f32 {
    (slope.atan().to_degrees() + degrees)
        .clamp(-MAX_ANGLE, MAX_ANGLE)
        .to_radians()
        .tan()
}

/// The curve's miniature in `rect`: its line over its range (at least 0..1).
pub(super) fn curve_preview(painter: &egui::Painter, rect: Rect, value: &Value) {
    painter.rect_filled(rect, 3, BG_PANEL);
    painter.rect_stroke(rect, 3, Stroke::new(1.0, STROKE), StrokeKind::Inside);
    if let Ok(points) = aschema::curve_points(value) {
        miniature(painter, rect, &points);
    }
}

fn miniature(painter: &egui::Painter, rect: Rect, points: &[[f32; 4]]) {
    let samples: Vec<f32> = (0..=40)
        .map(|i| aschema::evaluate_curve(points, i as f32 / 40.0))
        .collect();
    let lo = samples.iter().copied().fold(0.0f32, f32::min);
    let hi = samples.iter().copied().fold(1.0f32, f32::max);
    let line: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, y)| {
            pos2(
                rect.left() + 3.0 + i as f32 / 40.0 * (rect.width() - 6.0),
                rect.bottom() - 3.0 - (y - lo) / (hi - lo).max(1e-6) * (rect.height() - 6.0),
            )
        })
        .collect();
    painter.add(egui::Shape::line(line, Stroke::new(1.5, ACCENT)));
}

/// The value as written: every number with three decimals.
fn encode(points: &[[f32; 4]]) -> Value {
    Value::Array(
        points
            .iter()
            .map(|p| {
                Value::Array(
                    p.iter()
                        .map(|&n| aschema::json_f32((n * 1e3).round() / 1e3))
                        .collect(),
                )
            })
            .collect(),
    )
}

/// The curve editor; edits `value` in place. See docs/features/curves.md#curve-editor.
pub(super) fn curve_editor(
    ui: &mut Ui,
    value: &mut Value,
    state: &mut CurveState,
    settings: &mut CurveSettings,
) {
    if value.as_array().is_some_and(Vec::is_empty) {
        *value = aschema::linear_curve();
    }
    let mut points = match aschema::curve_points(value) {
        Ok(points) => points,
        Err(e) => {
            ui.label(RichText::new(e).color(CHANNEL_R));
            if ui.button("Reset").clicked() {
                *value = aschema::linear_curve();
            }
            return;
        }
    };
    let before = points.clone();
    if state.point.is_some_and(|k| k >= points.len()) {
        state.point = None;
    }
    let choose = |state: &mut CurveState, point: Option<usize>| {
        if state.point != point {
            state.split = false;
            state.active_in = false;
        }
        state.point = point;
    };

    // Presets.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (name, preset) in PRESETS {
            let (rect, response) = ui.allocate_exact_size(vec2(36.0, 24.0), Sense::click());
            let painter = ui.painter();
            painter.rect_filled(rect, 3, BG_PANEL);
            let stroke = if response.hovered() {
                TEXT_WEAK
            } else {
                STROKE
            };
            painter.rect_stroke(rect, 3, Stroke::new(1.0, stroke), StrokeKind::Inside);
            miniature(painter, rect, preset);
            if response.on_hover_text(name).clicked() {
                points = preset.to_vec();
                choose(state, None);
            }
        }
    });
    ui.add_space(6.0);

    // The frame: the unit square with space around it, grown to the curve, kept while
    // dragging.
    let samples: Vec<f32> = (0..=120)
        .map(|i| aschema::evaluate_curve(&points, i as f32 / 120.0))
        .chain(points.iter().map(|p| p[1]))
        .collect();
    let lowest = samples.iter().copied().fold(f32::MAX, f32::min);
    let highest = samples.iter().copied().fold(f32::MIN, f32::max);
    let fitted = (
        (lowest - X_PAD).min(-Y_PAD).max(Y_LIMIT.0),
        (highest + X_PAD).max(1.0 + Y_PAD).min(Y_LIMIT.1),
    );
    let (lo, hi) = match (state.drag, state.frame) {
        (Some(_), Some(frame)) => frame,
        _ => fitted,
    };
    state.frame = Some((lo, hi));
    let plot_w = (1.0 + 2.0 * X_PAD) * UNIT;
    let total = vec2(LABELS_W + plot_w + GAP + LANE, (hi - lo) * UNIT + LABELS_H);
    let (full, _) = ui.allocate_exact_size(vec2(ui.available_width(), total.y), Sense::hover());
    let left = full.left() + ((full.width() - total.x) / 2.0).max(0.0);
    let plot = Rect::from_min_size(
        pos2(left + LABELS_W, full.top()),
        vec2(plot_w, (hi - lo) * UNIT),
    );
    let response = ui.interact(plot, ui.id().with("curve_plot"), Sense::click_and_drag());
    let to_screen = |x: f32, y: f32| {
        pos2(
            plot.left() + (x + X_PAD) * UNIT,
            plot.top() + (hi - y) * UNIT,
        )
    };
    let from_screen = |p: egui::Pos2| {
        (
            (p.x - plot.left()) / UNIT - X_PAD,
            hi - (p.y - plot.top()) / UNIT,
        )
    };
    // A handle's direction for slope `m`: toward (1, m) on screen, mirrored for `in`.
    let handle_end = |p: &[f32; 4], out: bool, length: f32| {
        let m = if out { p[3] } else { p[2] };
        let dir = vec2(1.0, -m).normalized() * length;
        to_screen(p[0], p[1]) + if out { dir } else { -dir }
    };
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let near_point = |points: &[[f32; 4]], at: egui::Pos2| {
        points
            .iter()
            .enumerate()
            .map(|(k, p)| (k, (to_screen(p[0], p[1]) - at).length()))
            .filter(|(_, d)| *d <= REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(k, _)| k)
    };
    let deletable =
        |points: &[[f32; 4]], k: usize| k > 0 && k + 1 < points.len() && points.len() > 2;
    let is_split = |state: &CurveState, points: &[[f32; 4]], k: usize| {
        !joined(&points[k]) || (state.split && state.point == Some(k))
    };

    // Mouse.
    if response.double_clicked()
        && let Some(at) = pointer
    {
        match near_point(&points, at) {
            Some(k) => {
                if deletable(&points, k) {
                    points.remove(k);
                    choose(state, None);
                }
            }
            None => {
                let x = from_screen(at).0.clamp(0.0, 1.0);
                let k = add_point_at(&mut points, x);
                choose(state, Some(k));
            }
        }
        state.drag = None;
    } else if ui.input(|i| i.pointer.primary_pressed())
        && response.hovered()
        && let Some(at) = pointer
    {
        let handle = state.point.and_then(|k| {
            [true, false]
                .into_iter()
                .find(|&out| (handle_end(&points[k], out, HANDLE) - at).length() <= REACH)
                .map(|out| (k, out))
        });
        if let Some((k, out)) = handle {
            if ui.input(|i| i.modifiers.shift) {
                state.split = true;
            }
            state.active_in = !out;
            state.drag = Some(CurveDrag::Handle(k, out));
        } else {
            let point = near_point(&points, at);
            choose(state, point);
            state.drag = point.map(CurveDrag::Point);
        }
    }
    match (state.drag, ui.input(|i| i.pointer.primary_down()), pointer) {
        (Some(CurveDrag::Point(k)), true, Some(at)) if k < points.len() => {
            let (x, y) = from_screen(at);
            move_point(&mut points, k, x, y);
        }
        (Some(CurveDrag::Handle(k, out)), true, Some(at)) if k < points.len() => {
            let v = at - to_screen(points[k][0], points[k][1]);
            // Toward its own side; straight up or down is the steepest slope, not infinite.
            let dx = if out { v.x.max(1e-3) } else { v.x.min(-1e-3) };
            let slope = rotate(-v.y / dx, 0.0);
            let split = is_split(state, &points, k);
            set_slope(&mut points[k], slope, out, split);
        }
        _ => state.drag = None,
    }

    // Keys, while the pointer is over the editor and nothing is typed.
    let hovered = state
        .area
        .is_some_and(|area| ui.rect_contains_pointer(area));
    if hovered && !ui.ctx().egui_wants_keyboard_input() {
        let pressed = |key: egui::Key| ui.input(|i| i.key_pressed(key));
        let n = points.len();
        if pressed(egui::Key::Space) {
            settings.playing = !settings.playing;
        }
        if pressed(egui::Key::R) {
            settings.rotate = !settings.rotate;
        }
        match state.point {
            None => {
                if pressed(egui::Key::CloseBracket) || pressed(egui::Key::Home) {
                    choose(state, Some(0));
                } else if pressed(egui::Key::OpenBracket) || pressed(egui::Key::End) {
                    choose(state, Some(n - 1));
                }
            }
            Some(k) => {
                let split = is_split(state, &points, k);
                if pressed(egui::Key::CloseBracket) {
                    if settings.rotate && split && !state.active_in {
                        state.active_in = true;
                    } else {
                        choose(state, Some((k + 1) % n));
                    }
                } else if pressed(egui::Key::OpenBracket) {
                    if settings.rotate && split && state.active_in {
                        state.active_in = false;
                    } else {
                        choose(state, Some((k + n - 1) % n));
                        state.active_in = settings.rotate;
                    }
                } else if pressed(egui::Key::Home) {
                    choose(state, Some(0));
                } else if pressed(egui::Key::End) {
                    choose(state, Some(n - 1));
                } else if pressed(egui::Key::S) {
                    toggle_split(state, &mut points, k);
                } else if pressed(egui::Key::A) {
                    let k = add_point_after(&mut points, k);
                    choose(state, Some(k));
                } else if pressed(egui::Key::Delete) || pressed(egui::Key::X) {
                    if deletable(&points, k) {
                        points.remove(k);
                        choose(state, None);
                    }
                } else {
                    let arrows = [
                        (egui::Key::ArrowLeft, -1.0, 0.0),
                        (egui::Key::ArrowRight, 1.0, 0.0),
                        (egui::Key::ArrowUp, 0.0, 1.0),
                        (egui::Key::ArrowDown, 0.0, -1.0),
                    ];
                    for (key, dx, dy) in arrows {
                        if !pressed(key) {
                            continue;
                        }
                        if settings.rotate {
                            // Up / left: counter-clockwise.
                            let turn = if dx < 0.0 || dy > 0.0 { 1.0 } else { -1.0 };
                            let out = !(split && state.active_in);
                            let p = points[k];
                            let slope =
                                rotate(p[if out { 3 } else { 2 }], turn * settings.angle_step);
                            set_slope(&mut points[k], slope, out, split);
                        } else {
                            let p = points[k];
                            move_point(
                                &mut points,
                                k,
                                p[0] + dx * settings.x_step,
                                p[1] + dy * settings.y_step,
                            );
                        }
                    }
                }
            }
        }
    }

    // The playhead.
    let now = ui.input(|i| i.time);
    if settings.playing {
        let dt = state.last.map_or(0.0, |last| (now - last).min(0.1));
        state.phase = ((f64::from(state.phase) + dt / PASS) % 1.0) as f32;
        state.last = Some(now);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(20));
    } else {
        state.last = None;
    }

    // Drawing: frame, unit square, grid and labels.
    let painter = ui.painter();
    painter.rect_filled(plot, 4, BG_PANEL);
    let square = Rect::from_two_pos(to_screen(0.0, 0.0), to_screen(1.0, 1.0));
    painter.rect_filled(square, 0, BG_HEADER);
    let clip = painter.with_clip_rect(plot);
    let tenths = |from: f32, to: f32| ((from * 10.0).ceil() as i32)..=((to * 10.0).floor() as i32);
    for i in tenths(-X_PAD, 1.0 + X_PAD) {
        let whole = i % 10 == 0;
        let color = if whole {
            TEXT_WEAK.gamma_multiply(0.5)
        } else {
            STROKE
        };
        let x = to_screen(i as f32 / 10.0, 0.0).x;
        clip.vline(x, plot.y_range(), Stroke::new(1.0, color));
        if i % 5 == 0 {
            painter.text(
                pos2(x, plot.bottom() + 2.0),
                Align2::CENTER_TOP,
                format!("{}", i as f32 / 10.0),
                FontId::proportional(11.0),
                TEXT_WEAK,
            );
        }
    }
    for i in tenths(lo, hi) {
        let whole = i % 10 == 0;
        let color = if whole {
            TEXT_WEAK.gamma_multiply(0.5)
        } else {
            STROKE
        };
        let y = to_screen(0.0, i as f32 / 10.0).y;
        clip.hline(plot.x_range(), y, Stroke::new(1.0, color));
        if i % 5 == 0 {
            painter.text(
                pos2(plot.left() - 4.0, y),
                Align2::RIGHT_CENTER,
                format!("{}", i as f32 / 10.0),
                FontId::proportional(11.0),
                TEXT_WEAK,
            );
        }
    }
    clip.rect_stroke(
        square,
        0,
        Stroke::new(1.0, TEXT_WEAK.gamma_multiply(0.4)),
        StrokeKind::Middle,
    );

    // The curve, every pixel across the frame.
    let line: Vec<egui::Pos2> = (0..=plot_w as usize)
        .map(|i| {
            let x = i as f32 / UNIT - X_PAD;
            to_screen(x, aschema::evaluate_curve(&points, x))
        })
        .collect();
    clip.add(egui::Shape::line(line, Stroke::new(2.0, ACCENT)));

    // The playhead and its lane.
    let lane = Rect::from_min_size(
        pos2(plot.right() + GAP, plot.top()),
        vec2(LANE, plot.height()),
    );
    painter.rect_filled(lane, 4, BG_HEADER);
    let x = state.phase;
    clip.vline(
        to_screen(x, 0.0).x,
        plot.y_range(),
        Stroke::new(1.0, CHANNEL_R),
    );
    let y = to_screen(0.0, aschema::evaluate_curve(&points, x))
        .y
        .clamp(lane.top() + 5.0, lane.bottom() - 5.0);
    painter.circle_filled(pos2(lane.center().x, y), 5.0, ACCENT);

    // Points, and the chosen one's handles.
    let rotating = settings.rotate || matches!(state.drag, Some(CurveDrag::Handle(..)));
    for (k, p) in points.iter().enumerate() {
        let at = to_screen(p[0], p[1]);
        if state.point != Some(k) {
            clip.circle_filled(at, 4.0, TEXT);
            continue;
        }
        let split = is_split(state, &points, k);
        if rotating {
            clip.circle_stroke(at, 9.0, Stroke::new(1.0, ACCENT));
        }
        for out in [false, true] {
            let active = rotating && (!split || out != state.active_in);
            let (length, color) = match (active, split) {
                (true, _) => (HANDLE_ACTIVE, ACCENT),
                (false, true) => (HANDLE, CHANNEL_B),
                (false, false) => (HANDLE, TEXT_WEAK),
            };
            let end = handle_end(p, out, length);
            clip.line_segment([at, end], Stroke::new(1.5, color));
            clip.circle_filled(end, 3.0, color);
        }
        clip.circle_filled(at, 6.0, ACCENT);
    }
    ui.add_space(6.0);

    // The readout and exact fields.
    if let Some(k) = state.point {
        let split = is_split(state, &points, k);
        let p = points[k];
        let slope = |v: f32, active: bool| {
            if rotating && active {
                format!("[{v:.3}]")
            } else {
                format!("{v:.3}")
            }
        };
        let both = !split;
        let mut text = format!(
            "point {} of {}  x {:.3}  y {:.3}  in {}  out {}",
            k + 1,
            points.len(),
            p[0],
            p[1],
            slope(p[2], both || state.active_in),
            slope(p[3], both || !state.active_in),
        );
        if split {
            text.push_str("  split");
        }
        ui.label(RichText::new(text).monospace().size(12.0).color(TEXT_WEAK));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let left = if k > 0 { points[k - 1][0] } else { 0.0 };
            let right = points.get(k + 1).map_or(1.0, |p| p[0]);
            for (i, label) in ["X", "Y", "In", "Out"].into_iter().enumerate() {
                ui.label(RichText::new(label).color(TEXT_WEAK));
                let mut field = egui::DragValue::new(&mut points[k][i])
                    .speed(0.01)
                    .fixed_decimals(3);
                if i == 0 {
                    field = field.range(left..=right);
                }
                ui.add_sized(vec2(50.0, 22.0), field);
            }
            // Joined slopes typed into one follow in the other.
            if !split && points[k][2] != p[2] {
                points[k][3] = points[k][2];
            } else if !split && points[k][3] != p[3] {
                points[k][2] = points[k][3];
            }
        });
    } else {
        ui.label(
            RichText::new(format!("{} points · press one to edit it", points.len()))
                .size(12.0)
                .color(TEXT_WEAK),
        );
    }

    // Buttons and steps.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let (icon, tip) = if settings.playing {
            (Icon::Pause, "Pause (Space)")
        } else {
            (Icon::Play, "Play (Space)")
        };
        if icon_button(ui, icon, tip, true).clicked() {
            settings.playing = !settings.playing;
        }
        if icon_button(ui, Icon::Plus, "Add point (A)", true).clicked() {
            let k = add_point_after(&mut points, state.point.unwrap_or(0));
            choose(state, Some(k));
        }
        let chosen = state.point;
        let can_delete = chosen.is_some_and(|k| deletable(&points, k));
        if icon_button(ui, Icon::Trash2, "Delete point (Delete)", can_delete).clicked()
            && let Some(k) = chosen
        {
            points.remove(k);
            choose(state, None);
        }
        let split = chosen.is_some_and(|k| is_split(state, &points, k));
        let (icon, tip) = if split {
            (Icon::Link2, "Join slopes (S)")
        } else {
            (Icon::Unlink2, "Split slopes (S)")
        };
        if icon_button(ui, icon, tip, chosen.is_some()).clicked()
            && let Some(k) = chosen
        {
            toggle_split(state, &mut points, k);
        }
        if icon_toggle(ui, Icon::RotateCw, "Rotation mode (R)", settings.rotate).clicked() {
            settings.rotate = !settings.rotate;
        }
    });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(RichText::new("Steps").color(TEXT_WEAK));
        for (label, value, speed, suffix) in [
            ("x", &mut settings.x_step, 0.001, ""),
            ("y", &mut settings.y_step, 0.001, ""),
            ("angle", &mut settings.angle_step, 0.1, "°"),
        ] {
            ui.label(RichText::new(label).color(TEXT_WEAK));
            let field = egui::DragValue::new(value)
                .speed(speed)
                .range(0.001..=90.0)
                .max_decimals(3)
                .suffix(suffix);
            ui.add_sized(vec2(52.0, 22.0), field);
        }
    });
    state.area = Some(ui.min_rect());

    if points != before {
        *value = encode(&points);
    }
}

/// Moves point `k` to `x` (clamped between its neighbours and to 0..1) and `y`.
fn move_point(points: &mut [[f32; 4]], k: usize, x: f32, y: f32) {
    let left = if k > 0 { points[k - 1][0] } else { 0.0 };
    let right = points.get(k + 1).map_or(1.0, |p| p[0]);
    points[k][0] = x.clamp(left, right).clamp(0.0, 1.0);
    points[k][1] = y;
}

/// Sets the out (or in) slope; joined points turn both.
fn set_slope(p: &mut [f32; 4], slope: f32, out: bool, split: bool) {
    if out || !split {
        p[3] = slope;
    }
    if !out || !split {
        p[2] = slope;
    }
}

/// Splits a joined point (only marks it), or joins a split one (`in` takes `out`).
fn toggle_split(state: &mut CurveState, points: &mut [[f32; 4]], k: usize) {
    if joined(&points[k]) && !state.split {
        state.split = true;
        state.active_in = false;
    } else {
        points[k][2] = points[k][3];
        state.split = false;
        state.active_in = false;
    }
}

/// Adds a point on the curve at `x`, keeping its shape; returns its index.
fn add_point_at(points: &mut Vec<[f32; 4]>, x: f32) -> usize {
    let y = aschema::evaluate_curve(points, x);
    let slope = aschema::curve_slope(points, x);
    let k = points.partition_point(|p| p[0] <= x);
    points.insert(k, [x, y, slope, slope]);
    k
}

/// Adds a point halfway between point `k` and the next (from the last: the previous).
fn add_point_after(points: &mut Vec<[f32; 4]>, k: usize) -> usize {
    let k = k.min(points.len() - 1);
    let other = if k + 1 < points.len() {
        k + 1
    } else {
        k.saturating_sub(1)
    };
    let mid = (points[k][0] + points[other][0]) / 2.0;
    add_point_at(points, mid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_points_keeps_the_shape() {
        let mut points = vec![[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 2.0, 2.0]];
        let before: Vec<f32> = (0..=10)
            .map(|i| aschema::evaluate_curve(&points, i as f32 / 10.0))
            .collect();
        assert_eq!(add_point_after(&mut points, 0), 1);
        assert_eq!(points[1][0], 0.5);
        // From the last point: halfway back.
        assert_eq!(add_point_after(&mut points, 2), 2);
        assert_eq!(points[2][0], 0.75);
        for (i, y) in before.iter().enumerate() {
            let after = aschema::evaluate_curve(&points, i as f32 / 10.0);
            assert!((after - y).abs() < 1e-4, "{i}: {after} vs {y}");
        }
    }

    #[test]
    fn rotation_is_clamped() {
        assert!((rotate(0.0, 45.0) - 1.0).abs() < 1e-5);
        assert!((rotate(1.0, -45.0)).abs() < 1e-5);
        assert!((rotate(50.0, 30.0) - 89f32.to_radians().tan()).abs() < 1e-2);
        assert!(rotate(-50.0, -30.0) < -57.0);
    }

    #[test]
    fn split_and_join() {
        let mut state = CurveState {
            point: Some(0),
            ..Default::default()
        };
        let mut points = vec![[0.0, 0.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]];
        toggle_split(&mut state, &mut points, 0);
        assert!(state.split);
        assert_eq!(points[0], [0.0, 0.0, 1.0, 1.0]);
        set_slope(&mut points[0], 3.0, true, true);
        assert_eq!(points[0], [0.0, 0.0, 1.0, 3.0]);
        toggle_split(&mut state, &mut points, 0);
        assert_eq!(points[0], [0.0, 0.0, 3.0, 3.0]);
        set_slope(&mut points[0], 2.0, false, false);
        assert_eq!(points[0], [0.0, 0.0, 2.0, 2.0]);
    }

    #[test]
    fn presets_are_curves() {
        for (name, preset) in PRESETS {
            let value = encode(preset);
            let points = aschema::curve_points(&value).unwrap();
            assert_eq!(aschema::evaluate_curve(&points, 1.0), 1.0, "{name}");
        }
    }
}
