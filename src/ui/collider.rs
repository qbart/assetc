//! The Collider tab and the collider's preview; see docs/features/colliders.md#collider-tab.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use bevy_egui::egui::{self, Frame, Margin, RichText, Ui, vec2};

use super::{placeholder, prop_row_colored, property_style, set_color};
use crate::acollider::{self, KEYS};
use crate::icons::Icon;
use crate::theme::{CHANNEL_R, TEXT, TEXT_WEAK};
use assetc_collider::{Collider, Settings};

/// A model's `.acollider` sidecar being edited.
pub struct ColliderEdit {
    /// Its text on disk; `None` without a sidecar.
    text: Option<String>,
    /// Each setting of [`KEYS`] the sidecar sets: being edited and as saved.
    values: [Option<f64>; 4],
    saved: [Option<f64>; 4],
    /// Why the sidecar cannot be read.
    error: Option<String>,
}

/// What the tab asks for.
pub enum Change {
    /// Write this text to the sidecar (creating it).
    Write(String),
    /// Delete the sidecar.
    Remove,
}

impl ColliderEdit {
    pub fn read(model: &Path) -> Self {
        let text = std::fs::read_to_string(acollider::path_for(model)).ok();
        let mut values = [None; 4];
        let mut error = None;
        if let Some(text) = &text {
            match acollider::parse(text) {
                Ok(settings) => {
                    let set = acollider::keys_set(text);
                    for (i, k) in KEYS.iter().enumerate() {
                        if set.contains(&k.key) {
                            values[i] = Some(acollider::value(&settings, i));
                        }
                    }
                }
                Err(e) => error = Some(e),
            }
        }
        Self {
            text,
            values,
            saved: values,
            error,
        }
    }

    /// The settings the collider is made with: `None` without a (valid) sidecar.
    pub fn settings(&self) -> Option<Settings> {
        if self.text.is_none() || self.error.is_some() {
            return None;
        }
        let mut s = Settings::default();
        for (i, v) in self.values.iter().enumerate() {
            if let Some(v) = *v {
                match i {
                    0 => s.max_hulls = v as u32,
                    1 => s.max_vertices = v as u32,
                    2 => s.error = v as f32,
                    _ => s.resolution = v as u32,
                }
            }
        }
        Some(s)
    }

    pub fn exists(&self) -> bool {
        self.text.is_some()
    }
}

/// A collider made in the background for the preview.
#[derive(Default)]
pub struct ColliderJob {
    /// The last one made.
    done: Option<Made>,
    running: Option<Running>,
}

struct Running {
    settings: Settings,
    stamp: Option<SystemTime>,
    started: Instant,
    /// Filled by its thread once made.
    out: Arc<Mutex<Option<Result<Collider, String>>>>,
}

impl ColliderJob {
    /// The collider shown, once made.
    pub fn collider(&self) -> Option<&Collider> {
        self.done.as_ref().and_then(|(_, _, r, _)| r.as_ref().ok())
    }
}

/// A collider made: with which settings, for which version of the model, the collider (or why
/// not) and how long it took.
type Made = (
    Settings,
    Option<SystemTime>,
    Result<Collider, String>,
    Duration,
);

/// Colliders made for the preview, by model path.
pub type Jobs = HashMap<PathBuf, ColliderJob>;

/// Takes a finished collider in and starts one for `settings` when the one shown was made with
/// others (or for another version of the model), one at a time; see
/// docs/features/colliders.md#3d-view.
pub fn poll(jobs: &mut Jobs, model: &Path, settings: Settings, ctx: &egui::Context) {
    let job = jobs.entry(model.to_path_buf()).or_default();
    if let Some(running) = &job.running {
        let finished = running.out.lock().ok().and_then(|mut out| out.take());
        match finished {
            Some(result) => {
                job.done = Some((
                    running.settings,
                    running.stamp,
                    result,
                    running.started.elapsed(),
                ));
                job.running = None;
            }
            None => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
        }
    }
    let stamp = std::fs::metadata(model).and_then(|m| m.modified()).ok();
    let current = job
        .done
        .as_ref()
        .is_some_and(|(s, t, _, _)| *s == settings && *t == stamp);
    if current {
        return;
    }
    let out = Arc::new(Mutex::new(None));
    let file = model.to_path_buf();
    let filled = out.clone();
    std::thread::spawn(move || {
        // A panic is an error too, not a preview that never ends.
        let made = std::panic::catch_unwind(|| acollider::make(&file, &settings))
            .unwrap_or_else(|_| Err("making the collider failed".into()));
        if let Ok(mut slot) = filled.lock() {
            *slot = Some(made);
        }
    });
    job.running = Some(Running {
        settings,
        stamp,
        started: Instant::now(),
        out,
    });
    ctx.request_repaint_after(Duration::from_millis(100));
}

/// Each hull's edges, for the wireframes.
pub fn segments(collider: &Collider) -> Vec<(usize, Vec<[bevy::math::Vec3; 2]>)> {
    collider
        .hulls
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let edges = h
                .hull
                .edges()
                .into_iter()
                .map(|[a, b]| [h.hull.points[a], h.hull.points[b]].map(bevy::math::Vec3::from))
                .collect();
            (i, edges)
        })
        .collect()
}

/// The color of hull `i`: the ID modes' golden-ratio hues.
pub fn color(i: usize) -> egui::Color32 {
    let hue = (i as f32 * 0.618_034).fract();
    egui::ecolor::Hsva::new(hue, 0.65, 0.9, 1.0).into()
}

/// The Collider tab; returns what to write once a change is done.
pub fn tab(
    ui: &mut Ui,
    edit: &mut ColliderEdit,
    job: Option<&ColliderJob>,
    name: &str,
) -> Option<Change> {
    let mut change = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                if !edit.exists() {
                    ui.label(
                        RichText::new(
                            "No collider. A collider is a few convex hulls that follow the model, for physics.",
                        )
                        .color(TEXT_WEAK),
                    );
                    ui.add_space(8.0);
                    let make = egui::Button::image_and_text(
                        Icon::Plus.image(14.0, TEXT),
                        "Make collider",
                    );
                    if ui.add(make).clicked() {
                        change = Some(Change::Write(String::new()));
                    }
                    return;
                }
                if let Some(e) = &edit.error {
                    ui.label(RichText::new(format!("{name}.acollider: {e}")).color(CHANNEL_R));
                    ui.add_space(8.0);
                    if remove_button(ui) {
                        change = Some(Change::Remove);
                    }
                    return;
                }
                property_style(ui);
                let defaults = Settings::default();
                for (i, key) in KEYS.iter().enumerate() {
                    let default = acollider::value(&defaults, i);
                    let value = &mut edit.values[i];
                    prop_row_colored(ui, key.label, set_color(value.is_some()), Some(key.hint), |ui, width| {
                        let mut v = value.unwrap_or(default);
                        let before = v;
                        if i == 2 {
                            // Error: a share, shown as a percentage.
                            let slider = egui::Slider::new(&mut v, key.min..=key.max)
                                .logarithmic(true)
                                .custom_formatter(|v, _| format!("{:.1}%", v * 100.0))
                                .custom_parser(|s| {
                                    s.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|p| p / 100.0)
                                });
                            ui.spacing_mut().slider_width = (width - 64.0).max(40.0);
                            ui.add(slider);
                        } else {
                            let field = egui::DragValue::new(&mut v)
                                .range(key.min..=key.max)
                                .speed(1.0)
                                .max_decimals(0);
                            ui.add_sized(vec2(width, 22.0), field);
                        }
                        if v != before {
                            *value = Some(if i == 2 { (v * 10000.0).round() / 10000.0 } else { v.round() });
                        }
                    });
                }
                ui.add_space(8.0);
                result(ui, job);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let set = edit.values.iter().any(Option::is_some);
                    if ui.add_enabled(set, egui::Button::new("Use defaults")).clicked() {
                        edit.values = [None; 4];
                    }
                    if remove_button(ui) {
                        change = Some(Change::Remove);
                    }
                });
            });
        });
    if change.is_some() {
        return change;
    }
    // Written once nothing is dragged or typed into.
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    if busy || edit.values == edit.saved || edit.error.is_some() {
        return None;
    }
    let mut text = edit.text.clone().unwrap_or_default();
    for (i, (now, before)) in edit.values.iter().zip(edit.saved).enumerate() {
        if *now == before {
            continue;
        }
        text = match now {
            Some(v) => acollider::with_value(&text, i, *v),
            None => acollider::without_value(&text, i),
        }
        .unwrap_or(text);
    }
    edit.saved = edit.values;
    edit.text = Some(text.clone());
    Some(Change::Write(text))
}

fn remove_button(ui: &mut Ui) -> bool {
    let remove = egui::Button::image_and_text(Icon::Trash2.image(14.0, TEXT), "Remove collider");
    ui.add(remove).clicked()
}

/// The preview's result, or that it is being made.
fn result(ui: &mut Ui, job: Option<&ColliderJob>) {
    let running = job.is_some_and(|j| j.running.is_some());
    match job.and_then(|j| j.done.as_ref()) {
        _ if running => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0).color(TEXT_WEAK));
                ui.label(RichText::new("Making the collider…").color(TEXT_WEAK));
            });
        }
        Some((_, _, Ok(c), took)) => {
            let n = c.hulls.len();
            ui.label(
                RichText::new(format!(
                    "{n} hull{}, {} points",
                    if n == 1 { "" } else { "s" },
                    c.points()
                ))
                .color(TEXT),
            );
            let share = if c.model_volume > 0.0 {
                c.volume() / c.model_volume * 100.0
            } else {
                0.0
            };
            ui.label(
                RichText::new(format!(
                    "Hulls hold {share:.0}% of the model's volume · {:.2} s",
                    took.as_secs_f32()
                ))
                .color(TEXT_WEAK),
            );
        }
        Some((_, _, Err(e), _)) => {
            ui.label(RichText::new(e).color(CHANNEL_R));
        }
        None => placeholder(ui, "Making the collider…"),
    }
}
