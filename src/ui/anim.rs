//! A compiled `.aanim` played on its companion mesh, with a read-only timeline; see
//! docs/features/models.md#compiled-animation-preview.

use super::*;
use crate::aanim::{self, Decoded};

/// The timeline panel's height.
const PANEL: f32 = 132.0;
/// How long a passed event's marker flashes, in seconds.
const FLASH: f64 = 0.25;

/// Playback of a compiled animation.
pub(super) struct AnimPlayer {
    clip: usize,
    time: f32,
    playing: bool,
    /// When `time` last moved.
    last: Option<f64>,
    /// The clip and time last posed.
    posed: Option<(usize, f32)>,
    /// Events that flash, by index, since when.
    flashes: HashMap<usize, f64>,
}

impl Default for AnimPlayer {
    fn default() -> Self {
        Self {
            clip: 0,
            time: 0.0,
            playing: true,
            last: None,
            posed: None,
            flashes: HashMap::new(),
        }
    }
}

/// The 3D view of the posed mesh above, the timeline below.
pub(super) fn canvas_compiled_animation(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    preview: &mut Preview3d,
    mesh_view: &mut MeshView,
    vision: &mut Option<Vision>,
) {
    let Some((anim, missing)) = inspected.inspection.compiled_animation.take() else {
        return;
    };
    let mut player = std::mem::take(&mut inspected.anim_player);
    let panel = Rect::from_min_max(pos2(rect.left(), rect.bottom() - PANEL), rect.max);
    let view = Rect::from_min_max(rect.min, pos2(rect.right(), panel.top()));
    timeline(ui, panel, &anim, &mut player);

    match &missing {
        Some(message) => {
            ui.painter().rect_filled(view, 0, BG_CENTER);
            ui.painter().text(
                view.center(),
                Align2::CENTER_CENTER,
                message,
                FontId::proportional(14.0),
                TEXT_WEAK,
            );
        }
        None => {
            // A new upload needs its pose again.
            if inspected.uploaded_lod != Some((0, None)) {
                player.posed = None;
            }
            canvas_compiled_mesh(
                ui,
                view,
                Some(inspected),
                preview,
                mesh_view,
                (0, None),
                vision,
            );
            let at = (player.clip, player.time);
            if player.posed != Some(at)
                && let (Some(mesh), Some(clip)) = (&inspected.compiled_mesh, anim.clips.get(at.0))
                && let Some(level) = mesh.levels.first()
            {
                let rest: Vec<aanim::Local> = mesh.skeleton.iter().map(|j| j.rest).collect();
                let locals = aanim::sample(clip, at.1, &rest);
                preview.compiled_pose = Some(crate::amesh::posed(level, &mesh.skeleton, &locals));
                player.posed = Some(at);
            }
        }
    }
    inspected.anim_player = player;
    inspected.inspection.compiled_animation = Some((anim, missing));
}

/// Header, ruler, track and a line of counts; see docs/features/models.md#compiled-animation-preview.
fn timeline(ui: &mut Ui, rect: Rect, anim: &Decoded, player: &mut AnimPlayer) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0, BG_PANEL);
    painter.hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, STROKE));
    let Some(clip) = anim.clips.get(player.clip) else {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "No clips",
            FontId::proportional(13.0),
            TEXT_WEAK,
        );
        return;
    };
    let duration = clip.duration();
    let inner = rect.shrink2(vec2(8.0, 4.0));
    let mut ui = ui.new_child(
        UiBuilder::new()
            .id_salt("compiled_timeline")
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    let ui = &mut ui;
    ui.spacing_mut().item_spacing.y = 0.0;

    // Playback: Space over the panel, `,` / `.` steps (Shift: finer), looping play.
    let now = ui.input(|i| i.time);
    let before = player.time;
    if ui.rect_contains_pointer(rect)
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(egui::Key::Space))
    {
        player.playing = !player.playing;
    }
    if !ui.ctx().egui_wants_keyboard_input() {
        let (back, forward, fine) = ui.input(|i| {
            let mut out = (false, false, false);
            for event in &i.events {
                if let egui::Event::Key {
                    key: key @ (egui::Key::Comma | egui::Key::Period),
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    out.0 |= *key == egui::Key::Comma;
                    out.1 |= *key == egui::Key::Period;
                    out.2 |= modifiers.shift;
                }
            }
            out
        });
        if back || forward {
            let step = if fine { 0.001 } else { 0.01 };
            player.time = step_time(player.time, step, forward).clamp(0.0, duration);
            player.playing = false;
        }
    }
    let mut played = false;
    if player.playing {
        let dt = player.last.map_or(0.0, |last| (now - last).min(0.1)) as f32;
        player.time = if duration > 0.0 {
            (player.time + dt) % duration
        } else {
            0.0
        };
        player.last = Some(now);
        played = true;
        ui.ctx().request_repaint();
    } else {
        player.last = None;
    }

    // Header: play, time, clip.
    ui.allocate_ui_with_layout(
        vec2(inner.width(), 28.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (icon, tip) = if player.playing {
                (Icon::Pause, "Pause (Space)")
            } else {
                (Icon::Play, "Play (Space)")
            };
            if icon_button(ui, icon, tip, true).clicked() {
                player.playing = !player.playing;
            }
            ui.label(
                RichText::new(format!("{:.3} / {duration:.3} s", player.time))
                    .monospace()
                    .color(TEXT),
            );
            let mut chosen = player.clip;
            egui::ComboBox::from_id_salt("compiled_clip")
                .width(200.0)
                .selected_text(&clip.name)
                .show_ui(ui, |ui| {
                    for (i, c) in anim.clips.iter().enumerate() {
                        ui.selectable_value(&mut chosen, i, &c.name);
                    }
                });
            if chosen != player.clip {
                *player = AnimPlayer {
                    clip: chosen,
                    ..AnimPlayer::default()
                };
            }
        },
    );

    let (x0, x1) = (inner.left() + 6.0, inner.right() - 6.0);
    let span = duration.max(1e-3);
    let x_of = |t: f32| x0 + (x1 - x0) * (t / span).clamp(0.0, 1.0);
    let time_of = |x: f32| ((x - x0) / (x1 - x0)).clamp(0.0, 1.0) * span;

    // Ruler: press or drag moves the playhead and pauses.
    let (ruler, ruler_response) =
        ui.allocate_exact_size(vec2(inner.width(), 20.0), Sense::click_and_drag());
    if (ruler_response.is_pointer_button_down_on() || ruler_response.clicked())
        && let Some(p) = ruler_response.interact_pointer_pos()
    {
        player.time = time_of(p.x).min(duration);
        player.playing = false;
    }
    let painter = ui.painter().clone();
    let per_second = (x1 - x0) / span;
    let step = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]
        .into_iter()
        .find(|s| s * per_second >= 60.0)
        .unwrap_or(30.0);
    let mut k = 0;
    while k as f32 * step <= duration + 1e-4 {
        let t = k as f32 * step;
        let x = x_of(t);
        painter.vline(
            x,
            (ruler.bottom() - 6.0)..=ruler.bottom(),
            Stroke::new(1.0, TEXT_WEAK),
        );
        painter.text(
            pos2(x + 3.0, ruler.top() + 2.0),
            Align2::LEFT_TOP,
            seconds_label(t),
            FontId::proportional(11.0),
            TEXT_WEAK,
        );
        k += 1;
    }

    // Track: the clip's events, flashing when passed.
    let (track, _) = ui.allocate_exact_size(vec2(inner.width(), 32.0), Sense::hover());
    painter.rect_filled(track, 4, BG_HEADER);
    let mine: Vec<usize> = (0..anim.events.len())
        .filter(|&i| anim.events[i].clip as usize == player.clip)
        .collect();
    if played {
        for &i in &mine {
            let t = anim.events[i].time;
            let passed = if player.time >= before {
                before < t && t <= player.time
            } else {
                t > before || t <= player.time
            };
            if passed {
                player.flashes.insert(i, now);
            }
        }
    }
    player.flashes.retain(|_, at| now - *at < FLASH);
    for &i in &mine {
        let event = &anim.events[i];
        let center = pos2(x_of(event.time), track.center().y);
        let color = if player.flashes.contains_key(&i) {
            ACCENT
        } else {
            TEXT
        };
        let r = 5.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                center + vec2(0.0, -r),
                center + vec2(r, 0.0),
                center + vec2(0.0, r),
                center + vec2(-r, 0.0),
            ],
            color,
            Stroke::NONE,
        ));
        painter.text(
            center + vec2(r + 3.0, 0.0),
            Align2::LEFT_CENTER,
            &event.name,
            FontId::proportional(11.0),
            TEXT_WEAK,
        );
        // Hover: name, time and properties.
        let mut tip = format!("{}\n{:.3} s", event.name, event.time);
        for (key, value) in &event.properties {
            let value = match value {
                crate::enrich::PropValue::Bool(b) => b.to_string(),
                crate::enrich::PropValue::Int(n) => n.to_string(),
                crate::enrich::PropValue::Float(f) => f.to_string(),
                crate::enrich::PropValue::Text(t) => format!("{t:?}"),
            };
            tip.push_str(&format!("\n{key} = {value}"));
        }
        let at = Rect::from_center_size(center, vec2(12.0, 12.0));
        ui.interact(at, ui.id().with(("event", i)), Sense::hover())
            .on_hover_text(tip);
    }
    if !player.flashes.is_empty() {
        ui.ctx().request_repaint();
    }

    // Playhead.
    let x = x_of(player.time);
    painter.vline(x, ruler.top()..=track.bottom(), Stroke::new(1.0, ACCENT));
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(x - 4.0, ruler.top()),
            pos2(x + 4.0, ruler.top()),
            pos2(x, ruler.top() + 6.0),
        ],
        ACCENT,
        Stroke::NONE,
    ));

    // Counts.
    let keys: usize = clip.channels.iter().map(|c| c.times.len()).sum();
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!(
            "{} channels · {keys} keys · {} events",
            clip.channels.len(),
            mine.len()
        ))
        .size(12.0)
        .color(TEXT_WEAK),
    );
}
