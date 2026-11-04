//! The Material tab and 3D preview of `.amat` files; see docs/features/materials.md#material-tab
//! and docs/features/materials.md#3d-preview.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy_egui::egui::{self, Align2, FontId, Frame, Margin, Rect, RichText, Sense, Ui, vec2};

use super::{
    Fold, Inspected, icon_button, placeholder, prop_row_colored, prop_section, property_style,
    set_color, shape_tools, slider_box, toggle_vision, toolbar, view_3d, vision_tools,
};
use crate::build;
use crate::color_picker::{self, Picker};
use crate::icons::Icon;
use crate::material::{AlphaMode, HeightMode, Material, NormalY, Slot, Source, TextureIds};
use crate::preview3d::{CubemapUpload, MaterialTextures, Preview3d, Shape};
use crate::texture::{Image, linear_to_srgb};
use crate::theme::{BG_BAR, STROKE, TEXT, TEXT_WEAK};
use crate::vision::Vision;

/// Longest side of a preview texture.
const MAX_SIDE: u32 = 2048;

/// What the editor and preview of an `.amat` need, read off the UI thread.
pub struct PreparedMaterial {
    source: Source,
    /// The slots' files by name, in the material's folder.
    convention: BTreeMap<Slot, String>,
    /// The input's images, as paths from the material's folder.
    choices: Vec<String>,
    /// The input's HDR environment maps, input-relative.
    lights: Vec<String>,
    textures: MaterialTextures,
    problems: Vec<String>,
}

/// Reads `file` (an `.amat` in `input`) and decodes its textures; also the faces of `light`
/// (an input-relative `.exr`) when one lights the preview.
pub fn prepare(
    input: &Path,
    file: &Path,
    light: Option<&str>,
) -> (Result<PreparedMaterial, String>, Option<Vec<Image>>) {
    let file_name = file.file_name().unwrap_or_default().to_string_lossy();
    let source = std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|text| Source::parse(&text))
        .map_err(|e| format!("Can't read {file_name}: {e}"));
    let mut problems = Vec::new();
    let faces = light.and_then(
        |light| match crate::inspect::hdr_faces(&input.join(light)) {
            Ok(faces) => Some(faces),
            Err(e) => {
                problems.push(format!("lighting: {e}"));
                None
            }
        },
    );
    let source = match source {
        Ok(source) => source,
        Err(e) => return (Err(e), faces),
    };
    let dir = file.parent().unwrap_or(input);
    let name = file_name
        .strip_suffix(crate::material::SOURCE_SUFFIX)
        .unwrap_or(&file_name);
    let convention = convention(dir, name);
    let mut files = Vec::new();
    walk(input, &mut files);
    let choices = files
        .iter()
        .filter(|p| is_image(p))
        .map(|p| crate::adata::relative(dir, p))
        .collect();
    let lights = files
        .iter()
        .filter(|p| crate::hdr::is_hdr(p))
        .filter_map(|p| build::relative(input, p).ok())
        .collect();
    let textures = textures(dir, &source, &convention, &mut problems);
    let prepared = PreparedMaterial {
        source,
        convention,
        choices,
        lights,
        textures,
        problems,
    };
    (Ok(prepared), faces)
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["png", "jpg", "jpeg", "tga"].contains(&e.to_ascii_lowercase().as_str()))
}

/// The files of `dir` filling material `name`'s slots by name: `<name>.<ext>` and
/// `<name>.<suffix>.<ext>`; see docs/features/materials.md#finding-materials.
fn convention(dir: &Path, name: &str) -> BTreeMap<Slot, String> {
    let mut slots = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return slots;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !build::is_texture_source(&path) {
            continue;
        }
        let Some(file) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let slot = if stem == name {
            Some(Slot::BaseColor)
        } else {
            stem.strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('.'))
                .and_then(Slot::from_suffix)
        };
        if let Some(slot) = slot {
            slots.insert(slot, file.to_owned());
        }
    }
    slots
}

/// Every file under `dir` but hidden ones and atlas folders (whose images are not built alone),
/// sorted.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if !crate::atlas::is_atlas(&path) {
                walk(&path, out);
            }
        } else {
            out.push(path);
        }
    }
}

/// The slots' textures as the build would use them; what cannot be used goes to `problems`.
fn textures(
    dir: &Path,
    source: &Source,
    convention: &BTreeMap<Slot, String>,
    problems: &mut Vec<String>,
) -> MaterialTextures {
    let mut load = |slot: Slot| {
        let file = source
            .textures
            .get(slot)
            .or(convention.get(&slot).map(String::as_str))?;
        match build::load_source(&dir.join(file)) {
            Ok((image, _)) => Some(image),
            Err(e) => {
                problems.push(format!("{}: {file}: {e}", slot.name()));
                None
            }
        }
    };
    let fit = |image: image::RgbaImage| {
        let (width, height) = image.dimensions();
        let image = Image {
            width,
            height,
            pixels: image.into_raw(),
        };
        crate::inspect::fit_within(image, MAX_SIDE)
    };
    let base_color = load(Slot::BaseColor).map(fit);
    let normal = load(Slot::Normal).map(|mut image| {
        if source.normal_y == Some(NormalY::Dx) {
            for pixel in image.pixels_mut() {
                pixel[1] = 255 - pixel[1];
            }
        }
        fit(image)
    });
    let emissive = load(Slot::Emissive).map(fit);
    let depth = load(Slot::Height).map(|mut image| {
        for pixel in image.pixels_mut() {
            let depth = 255 - pixel[0];
            *pixel = image::Rgba([depth, depth, depth, 255]);
        }
        fit(image)
    });
    // Packed like the build: occlusion R, roughness G, metallic B; see
    // docs/features/materials.md#orm-packing.
    let orm_slots = [Slot::Occlusion, Slot::Roughness, Slot::Metallic];
    let inputs: Vec<(usize, image::RgbaImage)> = orm_slots
        .into_iter()
        .enumerate()
        .filter_map(|(channel, slot)| Some((channel, load(slot)?)))
        .collect();
    let orm = match inputs.first().map(|(_, image)| image.dimensions()) {
        Some(size) if inputs.iter().any(|(_, image)| image.dimensions() != size) => {
            problems.push("occlusion, roughness, metallic: ORM inputs differ in size".into());
            None
        }
        Some((width, height)) => {
            let mut packed = image::RgbaImage::from_pixel(width, height, image::Rgba([255; 4]));
            for (channel, image) in &inputs {
                for (out, src) in packed.pixels_mut().zip(image.pixels()) {
                    out[*channel] = src[0];
                }
            }
            Some(fit(packed))
        }
        None => None,
    };
    MaterialTextures {
        base_color,
        normal,
        orm,
        emissive,
        depth,
    }
}

/// Whose color picker is open.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Picking {
    BaseColor,
    Emissive,
}

/// An `.amat` being edited in the Material tab.
pub struct MaterialEdit {
    edit: Source,
    /// As last written; `edit` is saved when it differs and nothing is being dragged or typed.
    saved: Source,
    convention: BTreeMap<Slot, String>,
    choices: Vec<String>,
    lights: Vec<String>,
    problems: Vec<String>,
    /// Not yet sent to the 3D view.
    textures: Option<MaterialTextures>,
    picker: Picker,
    picking: Option<Picking>,
}

impl MaterialEdit {
    /// The source being edited.
    pub(super) fn source_mut(&mut self) -> &mut Source {
        &mut self.edit
    }
}

impl MaterialEdit {
    /// The edit of a fresh inspection; `kept` (the previous one of the same file) stays when the
    /// file still holds what it saved, so edits in progress survive.
    pub fn new(prepared: PreparedMaterial, kept: Option<MaterialEdit>) -> Self {
        let PreparedMaterial {
            source,
            convention,
            choices,
            lights,
            textures,
            problems,
        } = prepared;
        let (edit, picker, picking) = match kept {
            Some(k) if k.saved == source => (k.edit, k.picker, k.picking),
            Some(k) => (source.clone(), k.picker, k.picking),
            None => (source.clone(), Picker::default(), None),
        };
        Self {
            edit,
            saved: source,
            convention,
            choices,
            lights,
            problems,
            textures: Some(textures),
            picker,
            picking,
        }
    }

    /// The slots that have a texture, by name or by `[textures]`.
    fn maps(&self) -> Vec<Slot> {
        Slot::ALL
            .into_iter()
            .filter(|s| self.edit.textures.get(*s).is_some() || self.convention.contains_key(s))
            .collect()
    }

    /// The material as edited, with its defaults filled in.
    fn shown(&self) -> Material {
        self.edit.material(&self.maps(), TextureIds::default())
    }
}

/// Every key of an `.amat`; returns the source to write once an edit is done. See
/// docs/features/materials.md#material-tab.
pub fn material_tab(ui: &mut Ui, inspected: &mut Inspected) -> Option<Source> {
    let edit = match &mut inspected.material {
        Some(Ok(edit)) => edit,
        Some(Err(e)) => {
            placeholder(ui, e);
            return None;
        }
        None => {
            placeholder(ui, "No material");
            return None;
        }
    };
    let defaults = Source::default().material(&edit.maps(), TextureIds::default());
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(8)).show(ui, |ui| {
                property_style(ui);
                surface(ui, edit, &defaults);
                let s = &mut edit.edit;
                section(ui, "Alpha", |ui| {
                    choice_row(
                        ui,
                        ["Mode", "alpha_mode"],
                        "Opaque ignores alpha. Mask drops pixels below the cutoff. Blend mixes pixels with what is behind them.",
                        &mut s.alpha_mode,
                        &[
                            (AlphaMode::Opaque, "Opaque"),
                            (AlphaMode::Mask, "Mask"),
                            (AlphaMode::Blend, "Blend"),
                        ],
                    );
                    if s.alpha_mode == Some(AlphaMode::Mask) {
                        unit_row(
                            ui,
                            ["Cutoff", "alpha_cutoff"],
                            "Pixels with alpha below this are not drawn.",
                            &mut s.alpha_cutoff,
                            defaults.alpha_cutoff,
                        );
                    }
                });
                section(ui, "Normal", |ui| {
                    number_row(
                        ui,
                        ["Scale", "normal_scale"],
                        "Strength of the normal map's bumps; 0 flattens them, negative turns them inside out.",
                        &mut s.normal_scale,
                        defaults.normal_scale,
                        (f32::MIN, 0.01),
                    );
                    choice_row(
                        ui,
                        ["Y direction", "normal_y"],
                        "Which way the normal map's green channel points. Maps from DirectX tools (Unreal, Substance's DirectX preset) need DirectX.",
                        &mut s.normal_y,
                        &[
                            (NormalY::Gl, "OpenGL (+Y up)"),
                            (NormalY::Dx, "DirectX (+Y down)"),
                        ],
                    );
                });
                section(ui, "Occlusion", |ui| {
                    unit_row(
                        ui,
                        ["Strength", "occlusion_strength"],
                        "How much the occlusion map darkens ambient light; 0 turns it off.",
                        &mut s.occlusion_strength,
                        defaults.occlusion_strength,
                    );
                });
                emission(ui, edit, &defaults);
                let s = &mut edit.edit;
                section(ui, "Height", |ui| {
                    choice_row(
                        ui,
                        ["Mode", "height_mode"],
                        "Parallax fakes depth in the shader. Displacement moves the vertices and needs a dense mesh.",
                        &mut s.height_mode,
                        &[
                            (HeightMode::Parallax, "Parallax"),
                            (HeightMode::Displacement, "Displacement"),
                        ],
                    );
                    number_row(
                        ui,
                        ["Scale", "height_scale"],
                        "How far white is from black, in mesh units.",
                        &mut s.height_scale,
                        defaults.height_scale,
                        (0.0, 0.001),
                    );
                    unit_row(
                        ui,
                        ["Midlevel", "height_midlevel"],
                        "The height map value that stays on the surface; darker goes in, lighter comes out.",
                        &mut s.height_midlevel,
                        defaults.height_midlevel,
                    );
                });
                textures_section(ui, edit);
            });
        });
    material_commit(ui, edit)
}

/// The source to write once nothing is being dragged or typed into.
pub(super) fn material_commit(ui: &Ui, edit: &mut MaterialEdit) -> Option<Source> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.clone()
    })
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    prop_section(
        ui,
        ("amat", title),
        Fold::Open,
        RichText::new(title),
        |_| {},
        |_, _| {},
        body,
    );
}

fn surface(ui: &mut Ui, edit: &mut MaterialEdit, defaults: &Material) {
    section(ui, "Surface", |ui| {
        let s = &mut edit.edit;
        let current = s.base_color.unwrap_or(defaults.base_color);
        if let Some(color) = color_row(
            ui,
            ["Base color", "base_color"],
            "Multiplies the base color texture. Alpha is opacity, used by Mask and Blend.",
            (current, s.base_color.is_some(), true),
            (&mut edit.picker, &mut edit.picking, Picking::BaseColor),
        ) {
            s.base_color = color;
        }
        unit_row(
            ui,
            ["Metallic", "metallic"],
            "0 is non-metal (plastic, wood, stone), 1 is bare metal. Multiplies the metallic map.",
            &mut s.metallic,
            defaults.metallic,
        );
        unit_row(
            ui,
            ["Roughness", "roughness"],
            "0 is mirror-smooth, 1 fully rough. Multiplies the roughness map.",
            &mut s.roughness,
            defaults.roughness,
        );
        let reset = key_row(
            ui,
            ["Double-sided", "double_sided"],
            "Draw the back of faces too, e.g. leaves, cloth, glass panes.",
            s.double_sided.is_some(),
            |ui, _| {
                let mut v = s.double_sided.unwrap_or_default();
                if ui.checkbox(&mut v, "").changed() {
                    s.double_sided = Some(v);
                }
            },
        );
        if reset {
            s.double_sided = None;
        }
    });
}

fn emission(ui: &mut Ui, edit: &mut MaterialEdit, defaults: &Material) {
    section(ui, "Emission", |ui| {
        let s = &mut edit.edit;
        let [r, g, b] = s.emissive.unwrap_or(defaults.emissive);
        if let Some(color) = color_row(
            ui,
            ["Color", "emissive"],
            "Light the surface gives off by itself. Multiplies the emissive map; black is none.",
            ([r, g, b, 1.0], s.emissive.is_some(), false),
            (&mut edit.picker, &mut edit.picking, Picking::Emissive),
        ) {
            s.emissive = color.map(|[r, g, b, _]| [r, g, b]);
        }
        number_row(
            ui,
            ["Strength", "emissive_strength"],
            "Multiplies the color; above 1 makes it glow brighter than white, for bloom.",
            &mut s.emissive_strength,
            defaults.emissive_strength,
            (0.0, 0.01),
        );
    });
}

/// A row editing one key: the label `ACCENT` when the file sets it (the key on hover), Reset
/// while set, and a help line below. Returns whether Reset was clicked.
fn key_row(
    ui: &mut Ui,
    [label, key]: [&str; 2],
    help: &str,
    set: bool,
    control: impl FnOnce(&mut Ui, f32),
) -> bool {
    let mut reset = false;
    prop_row_colored(ui, label, set_color(set), Some(key), |ui, _| {
        // The same width in every row, Reset or not.
        let width = (ui.available_width() - 32.0).max(24.0);
        control(ui, width);
        if set {
            reset = icon_button(ui, Icon::RotateCcw, "Reset to default", true).clicked();
        }
    });
    help_line(ui, help);
    reset
}

/// `add` in a box exactly `width` wide, so a control cannot grow past it.
fn fixed(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(
        vec2(width, 22.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(width);
            add(ui);
        },
    );
}

fn help_line(ui: &mut Ui, help: &str) {
    let text = RichText::new(help).size(12.0).color(TEXT_WEAK);
    ui.add(egui::Label::new(text).wrap());
    ui.add_space(2.0);
}

/// A `0..=1` value as a slider, 2 decimals.
fn unit_row(ui: &mut Ui, names: [&str; 2], help: &str, value: &mut Option<f32>, default: f32) {
    let reset = key_row(ui, names, help, value.is_some(), |ui, width| {
        let mut v = value.unwrap_or(default);
        let before = v;
        slider_box(ui, width, &mut v, 0.0..=1.0, Some(2));
        if v != before {
            *value = Some((v * 100.0).round() / 100.0);
        }
    });
    if reset {
        *value = None;
    }
}

/// A number from `min` up, dragged by `speed` per point.
fn number_row(
    ui: &mut Ui,
    names: [&str; 2],
    help: &str,
    value: &mut Option<f32>,
    default: f32,
    (min, speed): (f32, f64),
) {
    let reset = key_row(ui, names, help, value.is_some(), |ui, width| {
        let mut v = value.unwrap_or(default);
        let before = v;
        let field = egui::DragValue::new(&mut v)
            .range(min..=f32::MAX)
            .speed(speed)
            .max_decimals(4);
        ui.add_sized(vec2(width, 22.0), field);
        if v != before {
            *value = Some(v);
        }
    });
    if reset {
        *value = None;
    }
}

/// One of `options`, shown by name.
fn choice_row<T: Copy + PartialEq + Default>(
    ui: &mut Ui,
    names: [&str; 2],
    help: &str,
    value: &mut Option<T>,
    options: &[(T, &str)],
) {
    let current = value.unwrap_or_default();
    let name = |v: T| {
        options
            .iter()
            .find(|(o, _)| *o == v)
            .map_or("", |(_, n)| *n)
    };
    let reset = key_row(ui, names, help, value.is_some(), |ui, width| {
        fixed(ui, width, |ui| {
            egui::ComboBox::from_id_salt(("amat", names[1]))
                .width(width)
                .truncate()
                .selected_text(name(current))
                .show_ui(ui, |ui| {
                    for &(option, label) in options {
                        if ui.selectable_label(option == current, label).clicked() {
                            *value = Some(option);
                        }
                    }
                });
        });
    });
    if reset {
        *value = None;
    }
}

/// A linear color as a swatch that opens the picker below the row; `Some(new)` when it changed
/// (`Some(None)` on Reset). `alpha` edits alpha too.
fn color_row(
    ui: &mut Ui,
    names: [&str; 2],
    help: &str,
    (current, set, alpha): ([f32; 4], bool, bool),
    (picker, picking, own): (&mut Picker, &mut Option<Picking>, Picking),
) -> Option<Option<[f32; 4]>> {
    let mut rgba = to_srgb8(current);
    if !alpha {
        rgba[3] = 255;
    }
    let open = *picking == Some(own);
    let mut toggled = false;
    let reset = key_row(ui, names, help, set, |ui, width| {
        let (rect, response) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click());
        color_picker::checker(ui, rect, 4);
        let painter = ui.painter();
        painter.rect_filled(rect, 4, color_picker::to_color32(rgba));
        let stroke = if open || response.hovered() {
            TEXT
        } else {
            STROKE
        };
        painter.rect_stroke(
            rect,
            4,
            egui::Stroke::new(1.0, stroke),
            egui::StrokeKind::Inside,
        );
        let hex = crate::gradient::format_color(rgba);
        let text = if alpha { hex } else { hex[..7].to_owned() };
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::monospace(11.0),
            if rgba[..3].iter().map(|&c| u32::from(c)).sum::<u32>() > 384 {
                BG_BAR
            } else {
                TEXT
            },
        );
        toggled = response.on_hover_text("Pick a color").clicked();
    });
    if toggled {
        *picking = if open { None } else { Some(own) };
    }
    if reset {
        return Some(None);
    }
    if *picking == Some(own) && picker.show(ui, &mut rgba) {
        if !alpha {
            rgba[3] = 255;
        }
        ui.add_space(6.0);
        return Some(Some(to_linear(rgba, current)));
    }
    if *picking == Some(own) {
        ui.add_space(6.0);
    }
    None
}

fn to_srgb8(linear: [f32; 4]) -> [u8; 4] {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b, a] = linear;
    [
        byte(linear_to_srgb(r)),
        byte(linear_to_srgb(g)),
        byte(linear_to_srgb(b)),
        byte(a),
    ]
}

/// `rgba` (sRGB) as linear, rounded to 4 decimals; channels `rgba` did not change keep their
/// value in `before`, so a picker round trip does not move them.
fn to_linear(rgba: [u8; 4], before: [f32; 4]) -> [f32; 4] {
    let old = to_srgb8(before);
    std::array::from_fn(|i| {
        if rgba[i] == old[i] {
            return before[i];
        }
        let v = f32::from(rgba[i]) / 255.0;
        let v = if i == 3 {
            v
        } else if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        };
        (v * 1e4).round() / 1e4
    })
}

fn textures_section(ui: &mut Ui, edit: &mut MaterialEdit) {
    section(ui, "Textures", |ui| {
        help_line(
            ui,
            "Files named <name>.<suffix>.<ext> next to the material fill their slot by name. Choose another texture to use it instead.",
        );
        for slot in Slot::ALL {
            let set = edit.edit.textures.get(slot).map(str::to_owned);
            let by_name = edit.convention.get(&slot);
            let by_name_text = by_name.map_or("None".to_owned(), |f| format!("{f} (by name)"));
            let shown = match &set {
                Some(path) => RichText::new(path).color(TEXT),
                None => RichText::new(&by_name_text).color(TEXT_WEAK),
            };
            let hint = match slot.suffix() {
                Some(suffix) => format!("{}, by name: <name>.{suffix}.<ext>", slot.name()),
                None => format!("{}, by name: <name>.<ext>", slot.name()),
            };
            let mut chosen = None;
            prop_row_colored(
                ui,
                slot.label(),
                set_color(set.is_some()),
                Some(&hint),
                |ui, _| {
                    let width = (ui.available_width() - 32.0).max(24.0);
                    fixed(ui, width, |ui| {
                        egui::ComboBox::from_id_salt(("amat_texture", slot.name()))
                            .width(width)
                            .truncate()
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                if ui.selectable_label(set.is_none(), &by_name_text).clicked() {
                                    chosen = Some(None);
                                }
                                for choice in &edit.choices {
                                    let on = set.as_deref() == Some(choice.as_str());
                                    if ui.selectable_label(on, choice).clicked() {
                                        chosen = Some(Some(choice.clone()));
                                    }
                                }
                            });
                    });
                    if set.is_some()
                        && icon_button(ui, Icon::RotateCcw, "Reset to default", true).clicked()
                    {
                        chosen = Some(None);
                    }
                },
            );
            if let Some(path) = chosen {
                edit.edit.textures.set(slot, path);
            }
        }
    });
}

/// The material on a shape in 3D, lit by `light`; returns a newly chosen light. See
/// docs/features/materials.md#3d-preview.
pub fn canvas_material(
    ui: &mut Ui,
    rect: Rect,
    inspected: &mut Inspected,
    preview: &mut Preview3d,
    vision: &mut Option<Vision>,
    light: &Option<String>,
) -> Option<Option<String>> {
    match &mut inspected.material {
        Some(Ok(edit)) => {
            preview.material = Some(edit.shown());
            if let Some(textures) = edit.textures.take() {
                preview.material_textures = Some(textures);
            }
        }
        _ => {
            preview.material = Some(Source::default().material(&[], TextureIds::default()));
            if !inspected.uploaded_material {
                preview.material_textures = Some(MaterialTextures::default());
            }
        }
    }
    inspected.uploaded_material = true;
    if let Some(faces) = &inspected.cubemap
        && !inspected.uploaded_cubemap
    {
        preview.cubemap = Some(CubemapUpload {
            size: faces[0].width,
            pixels: faces
                .iter()
                .flat_map(|f| f.pixels.iter().copied())
                .collect(),
        });
        inspected.uploaded_cubemap = true;
    }
    preview.environment = light.is_some() && inspected.cubemap.is_some();
    view_3d(ui, rect, preview, true);

    let shapes = shape_tools(preview.material_shape());
    let seen = vision_tools(*vision);
    match toolbar(ui, rect, &[&shapes, &seen]) {
        Some((0, button)) => preview.material_shape = Some(Shape::ALL[button]),
        Some((_, button)) => toggle_vision(vision, button),
        None => {}
    }

    let Some(Ok(edit)) = &inspected.material else {
        return None;
    };
    // Problems, bottom-left.
    let painter = ui.painter_at(rect);
    for (i, problem) in edit.problems.iter().rev().enumerate() {
        painter.text(
            rect.left_bottom() + vec2(8.0, -8.0 - 18.0 * i as f32),
            Align2::LEFT_BOTTOM,
            problem,
            FontId::proportional(13.0),
            TEXT_WEAK,
        );
    }
    // Lighting, bottom-right.
    let mut chosen = None;
    let size = vec2(240.0, 32.0);
    let area = Rect::from_min_size(rect.right_bottom() - size - vec2(8.0, 8.0), size);
    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
        property_style(ui);
        Frame::new()
            .fill(BG_BAR)
            .corner_radius(6)
            .inner_margin(Margin::same(4))
            .show(ui, |ui| {
                egui::ComboBox::from_id_salt("amat_light")
                    .width(size.x - 8.0)
                    .truncate()
                    .selected_text(light.as_deref().unwrap_or("Studio"))
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(light.is_none(), "Studio").clicked() {
                            chosen = Some(None);
                        }
                        for choice in &edit.lights {
                            let on = light.as_deref() == Some(choice.as_str());
                            if ui.selectable_label(on, choice).clicked() {
                                chosen = Some(Some(choice.clone()));
                            }
                        }
                    });
            });
    });
    chosen.filter(|c| c != light)
}
