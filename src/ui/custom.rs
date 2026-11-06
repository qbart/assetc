//! The Shader tab and Layout view of `.ashader` files, and the Material tab and Material view
//! of custom materials (`.acmat`); see docs/features/custom-materials.md#gui.

use std::path::{Path, PathBuf};

use bevy_egui::egui::{self, Frame, Margin, RichText, Ui, vec2};
use serde_json::Map;

use super::data::{FieldsState, struct_fields};
use super::material_edit::{fixed, help_line, key_row, section};
use super::{icon_button, placeholder, property_style};
use crate::acmat::{self, TextureRef};
use crate::adata::{self, DataProblem, Target};
use crate::aschema::Schema;
use crate::icons::Icon;
use crate::material::NormalY;
use crate::shader::{
    self, Blend, Block, ColorKey, Cull, DepthTest, Dimension, Fallback, Pack, ParamsRef, Resource,
    Shader, State, Texture, Usage,
};
use crate::theme::{ACCENT, CHANNEL_R, TEXT, TEXT_WEAK};

// Shaders.

/// What a shader's stages and params came to: its block and the resources found, or why not.
pub struct Checked {
    pub block: Block,
    pub resources: Vec<Resource>,
    pub stages: (bool, bool),
}

/// An `.ashader` being edited.
pub struct ShaderEdit {
    pub edit: Shader,
    pub saved: Shader,
    /// Input-relative.
    rel: String,
    input: PathBuf,
    /// Schemas of the input relative to the shader's folder, with their structs.
    schemas: Vec<(String, Vec<String>)>,
    /// `.spv` files of the input relative to the shader's folder.
    spirv: Vec<String>,
    /// The shader [`Self::checked`] was computed for.
    checked_for: Option<Shader>,
    checked: Result<Checked, String>,
    /// The edit breaks a rule of the format; it is not saved.
    invalid: Option<String>,
    code: CodeView,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum CodeView {
    #[default]
    Glsl,
    Wgsl,
}

/// Reads `file` and what its tab offers; off the UI thread.
pub fn prepare_shader(input: &Path, file: &Path) -> Result<ShaderEdit, String> {
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let shader = Shader::parse(&text)?;
    let rel = crate::build::relative(input, file)?;
    let dir = file.parent().unwrap_or(input);
    let schemas = adata::schemas_in(input)
        .into_iter()
        .map(|path| {
            let structs = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| Schema::parse(&t).ok())
                .map(|s| {
                    s.types
                        .iter()
                        .filter(|t| matches!(t.body, crate::aschema::Body::Struct { .. }))
                        .map(|t| t.name.clone())
                        .collect()
                })
                .unwrap_or_default();
            (adata::relative(dir, &path), structs)
        })
        .collect();
    let mut spirv: Vec<String> = crate::build::files_under(input)
        .into_iter()
        .filter(|f| shader::is_spirv(f))
        .map(|f| adata::relative(dir, &f))
        .collect();
    spirv.sort();
    let mut edit = ShaderEdit {
        edit: shader.clone(),
        saved: shader,
        rel,
        input: input.to_path_buf(),
        schemas,
        spirv,
        checked_for: None,
        checked: Err(String::new()),
        invalid: None,
        code: CodeView::default(),
    };
    edit.recheck();
    Ok(edit)
}

impl ShaderEdit {
    /// Keeps the session state of `kept` (the same file before).
    pub fn keep(mut self, kept: Option<ShaderEdit>) -> Self {
        if let Some(kept) = kept {
            self.code = kept.code;
        }
        self
    }

    fn recheck(&mut self) {
        if self.checked_for.as_ref() == Some(&self.edit) {
            return;
        }
        self.checked_for = Some(self.edit.clone());
        self.checked = shader::load_parsed(&self.input, &self.rel, self.edit.clone()).map(|l| {
            Checked {
                stages: (l.vertex.is_some(), l.fragment.is_some()),
                block: l.block,
                resources: l.resources,
            }
        });
    }
}

/// The Shader tab; returns the file's new text when it should be saved.
pub fn shader_tab(ui: &mut Ui, edit: &mut ShaderEdit) -> Option<String> {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    property_style(ui);
                    shader_sections(ui, edit);
                });
        });
    shader_commit(ui, edit)
}

fn shader_sections(ui: &mut Ui, edit: &mut ShaderEdit) {
    if let Some(problem) = &edit.invalid {
        ui.label(RichText::new(problem).size(12.0).color(CHANNEL_R));
        ui.add_space(4.0);
    }
    section(ui, "Shader", |ui| {
        let doc = &mut edit.edit.doc;
        key_row(
            ui,
            ["Doc", "doc"],
            "One line about the shader, shown in the Inspector.",
            !doc.is_empty(),
            |ui, width| {
                ui.add_sized(vec2(width, 22.0), egui::TextEdit::singleline(doc));
            },
        )
        .then(|| doc.clear());
        params_row(ui, &mut edit.edit.params, &edit.schemas);
    });
    state_section(ui, &mut edit.edit.state, None, "ashader");
    section(ui, "Stages", |ui| {
        help_line(
            ui,
            "SPIR-V the build checks and puts in the .ashader.bin. Without a fragment stage there is no preview.",
        );
        let s = &mut edit.edit.stages;
        stage_row(ui, "Fragment", "fragment", &mut s.fragment, &edit.spirv);
        stage_row(ui, "Vertex", "vertex", &mut s.vertex, &edit.spirv);
    });
    textures_section(ui, &mut edit.edit.textures);
}

fn shader_commit(ui: &Ui, edit: &mut ShaderEdit) -> Option<String> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    if busy {
        return None;
    }
    edit.recheck();
    if edit.edit == edit.saved {
        edit.invalid = None;
        return None;
    }
    // Only a file the build can read is written.
    let text = edit.edit.to_toml();
    match Shader::parse(&text) {
        Ok(_) => {
            edit.invalid = None;
            edit.saved = edit.edit.clone();
            Some(text)
        }
        Err(e) => {
            edit.invalid = Some(format!("Not saved: {e}"));
            None
        }
    }
}

fn params_row(ui: &mut Ui, params: &mut Option<ParamsRef>, schemas: &[(String, Vec<String>)]) {
    let current = params
        .as_ref()
        .map_or("None".to_owned(), |p| format!("{} ({})", p.ty, p.schema));
    let reset = key_row(
        ui,
        ["Params", "params"],
        "The material's values: a struct of a schema. Its numbers and colors make the GPU block.",
        params.is_some(),
        |ui, width| {
            fixed(ui, width, |ui| {
                egui::ComboBox::from_id_salt("ashader_params")
                    .width(width)
                    .truncate()
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if schemas.is_empty() {
                            ui.weak("No schemas in the input");
                        }
                        for (path, structs) in schemas {
                            ui.menu_button(path, |ui| {
                                if structs.is_empty() {
                                    ui.weak("No structs");
                                }
                                for ty in structs {
                                    if ui.button(ty).clicked() {
                                        *params = Some(ParamsRef {
                                            schema: path.clone(),
                                            ty: ty.clone(),
                                        });
                                        ui.close();
                                    }
                                }
                            });
                        }
                    });
            });
        },
    );
    if reset {
        *params = None;
    }
}

fn stage_row(ui: &mut Ui, label: &str, key: &str, value: &mut Option<String>, files: &[String]) {
    let current = value.clone().unwrap_or_else(|| "None".into());
    let reset = key_row(
        ui,
        [label, key],
        "",
        value.is_some(),
        |ui, width| {
            fixed(ui, width, |ui| {
                egui::ComboBox::from_id_salt(("ashader_stage", key))
                    .width(width)
                    .truncate()
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(value.is_none(), "None").clicked() {
                            *value = None;
                        }
                        for f in files {
                            if ui.selectable_label(value.as_ref() == Some(f), f).clicked() {
                                *value = Some(f.clone());
                            }
                        }
                    });
            });
        },
    );
    if reset {
        *value = None;
    }
}

/// A drop-down of `options`; `value` `None` shows `default`'s name in `TEXT_WEAK`.
fn option_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    names: [&str; 2],
    help: &str,
    value: &mut Option<T>,
    (default, options): (T, &[(T, &str)]),
    salt: &str,
) {
    let name = |v: T| {
        options
            .iter()
            .find(|(o, _)| *o == v)
            .map_or("", |(_, n)| *n)
    };
    let text = match *value {
        Some(v) => RichText::new(name(v)),
        None => RichText::new(name(default)).color(TEXT_WEAK),
    };
    let reset = key_row(ui, names, help, value.is_some(), |ui, width| {
        fixed(ui, width, |ui| {
            egui::ComboBox::from_id_salt((salt, names[1]))
                .width(width)
                .truncate()
                .selected_text(text)
                .show_ui(ui, |ui| {
                    for &(option, label) in options {
                        if ui.selectable_label(*value == Some(option), label).clicked() {
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

/// A bool key with a checkbox; `default` shown while unset.
fn bool_row(
    ui: &mut Ui,
    names: [&str; 2],
    help: &str,
    value: &mut Option<bool>,
    default: bool,
) {
    let reset = key_row(ui, names, help, value.is_some(), |ui, _| {
        let mut v = value.unwrap_or(default);
        if ui.checkbox(&mut v, "").changed() {
            *value = Some(v);
        }
    });
    if reset {
        *value = None;
    }
}

/// The render state; `base` is the shader's, which a material's overrides start from.
fn state_section(ui: &mut Ui, state: &mut State, base: Option<&State>, salt: &str) {
    let base = base.copied().unwrap_or_default();
    let shown = base.with(*state);
    section(ui, "State", |ui| {
        let blends: Vec<(Blend, &str)> = Blend::ALL.iter().map(|b| (*b, b.key())).collect();
        option_row(
            ui,
            ["Blend", "blend"],
            "Opaque ignores alpha, Mask drops pixels below the cutoff, Blend mixes with what is behind, Add and Multiply combine with it.",
            &mut state.blend,
            (base.blend(), &blends),
            salt,
        );
        if shown.blend() == Blend::Mask {
            let reset = key_row(
                ui,
                ["Alpha cutoff", "alpha_cutoff"],
                "Pixels with alpha below this are not drawn.",
                state.alpha_cutoff.is_some(),
                |ui, width| {
                    let mut v = state.alpha_cutoff.unwrap_or(base.alpha_cutoff());
                    let before = v;
                    super::slider_box(ui, width, &mut v, 0.0..=1.0, Some(2));
                    if v != before {
                        state.alpha_cutoff = Some((v * 100.0).round() / 100.0);
                    }
                },
            );
            if reset {
                state.alpha_cutoff = None;
            }
        }
        let culls: Vec<(Cull, &str)> = Cull::ALL.iter().map(|c| (*c, c.key())).collect();
        option_row(
            ui,
            ["Cull", "cull"],
            "Which faces are not drawn; none draws both sides.",
            &mut state.cull,
            (base.cull(), &culls),
            salt,
        );
        let depth_default = State {
            blend: Some(shown.blend()),
            depth_write: None,
            ..base
        }
        .depth_write();
        bool_row(
            ui,
            ["Depth write", "depth_write"],
            "Write depth, so what is behind is hidden. Off by default for transparent blends.",
            &mut state.depth_write,
            depth_default,
        );
        let tests: Vec<(DepthTest, &str)> =
            DepthTest::ALL.iter().map(|d| (*d, d.key())).collect();
        option_row(
            ui,
            ["Depth test", "depth_test"],
            "When a pixel is drawn against the depth already there.",
            &mut state.depth_test,
            (base.depth_test(), &tests),
            salt,
        );
    });
}

fn textures_section(ui: &mut Ui, textures: &mut Vec<Texture>) {
    let mut remove = None;
    let mut swap = None;
    let n = textures.len();
    section(ui, "Textures", |ui| {
        help_line(
            ui,
            "What the shader samples, in order. A material fills each by its name; packed ones take channels of other images.",
        );
        for (i, t) in textures.iter_mut().enumerate() {
            ui.push_id(("ashader_texture", i), |ui| {
                Frame::new()
                    .fill(crate::theme::BG_HEADER)
                    .corner_radius(6)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&t.name).monospace().color(TEXT));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if icon_button(ui, Icon::X, "Remove", true).clicked() {
                                        remove = Some(i);
                                    }
                                    if icon_button(ui, Icon::ChevronDown, "Move down", i + 1 < n)
                                        .clicked()
                                    {
                                        swap = Some((i, i + 1));
                                    }
                                },
                            );
                        });
                        texture_rows(ui, t);
                    });
                ui.add_space(8.0);
            });
        }
        if ui
            .add(egui::Button::image_and_text(
                Icon::Plus.image(14.0, TEXT),
                "Add texture",
            ))
            .clicked()
        {
            let mut k = textures.len() + 1;
            while textures.iter().any(|t| t.name == format!("texture{k}")) {
                k += 1;
            }
            textures.push(Texture::new(&format!("texture{k}"), Usage::Data));
        }
    });
    if let Some(i) = remove {
        textures.remove(i);
    }
    if let Some((a, b)) = swap {
        textures.swap(a, b);
    }
}

/// A row of a key every texture has: no Reset.
fn required_row(ui: &mut Ui, [label, key]: [&str; 2], help: &str, control: impl FnOnce(&mut Ui, f32)) {
    super::prop_row_colored(ui, label, ACCENT, Some(key), |ui, width| {
        control(ui, (width - 32.0).max(24.0));
    });
    help_line(ui, help);
}

fn texture_rows(ui: &mut Ui, t: &mut Texture) {
    required_row(
        ui,
        ["Name", "name"],
        "The material's key for it, and its by-name suffix: <material>.<name>.png.",
        |ui, width| {
            ui.add_sized(vec2(width, 22.0), egui::TextEdit::singleline(&mut t.name));
        },
    );
    required_row(
        ui,
        ["Usage", "usage"],
        "What it holds, and so which files fill it: images for color, data, normal and height; else that kind of asset.",
        |ui, width| {
            fixed(ui, width, |ui| {
                egui::ComboBox::from_id_salt("ashader_usage")
                    .width(width)
                    .selected_text(t.usage.key())
                    .show_ui(ui, |ui| {
                        for u in Usage::ALL {
                            ui.selectable_value(&mut t.usage, u, u.key());
                        }
                    });
            });
        },
    );
    let dims = [(Dimension::D2, "2d"), (Dimension::D3, "3d"), (Dimension::Cube, "cube")];
    let default_dim = Texture::new("", t.usage).dimension();
    option_row(
        ui,
        ["Dimension", "dimension"],
        "2D, 3D (volume LUTs) or a cubemap (environments).",
        &mut t.dimension,
        (default_dim, &dims),
        "ashader_tex",
    );
    if t.usage.takes_images() && t.pack.is_none() {
        let spaces = [(ColorKey::Srgb, "srgb"), (ColorKey::Linear, "linear")];
        let default = if t.usage == Usage::Color {
            ColorKey::Srgb
        } else {
            ColorKey::Linear
        };
        option_row(
            ui,
            ["Color space", "color_space"],
            "How an image is compiled: sRGB for colors, linear for data.",
            &mut t.color_space,
            (default, &spaces),
            "ashader_tex",
        );
    }
    let fallbacks = [
        Fallback::White,
        Fallback::Black,
        Fallback::Gray,
        Fallback::Normal,
        Fallback::Transparent,
    ]
    .map(|f| (f, f.key()));
    let default_fallback = Texture::new("", t.usage).fallback();
    option_row(
        ui,
        ["Default", "default"],
        "What the game binds when a material leaves it empty.",
        &mut t.default,
        (default_fallback, &fallbacks),
        "ashader_tex",
    );
    let mut required = Some(t.required).filter(|r| *r);
    bool_row(
        ui,
        ["Required", "required"],
        "A material without it fails to build.",
        &mut required,
        false,
    );
    t.required = required.unwrap_or(false);
    if t.pack.is_none() {
        let mut bare = Some(t.bare).filter(|b| *b);
        bool_row(
            ui,
            ["Bare", "bare"],
            "The material's unsuffixed image (lake.png) fills it by name. One texture at most.",
            &mut bare,
            false,
        );
        t.bare = bare.unwrap_or(false);
    }
    bool_row(
        ui,
        ["Mipmaps", "mipmaps"],
        "Off: compiled without mipmaps.",
        &mut t.mipmaps,
        true,
    );
    let mut packed = Some(t.pack.is_some()).filter(|p| *p);
    bool_row(
        ui,
        ["Packed", "pack"],
        "Built from channels of other images, the inputs, like ORM.",
        &mut packed,
        false,
    );
    match (packed.unwrap_or(false), &mut t.pack) {
        (true, None) => {
            t.pack = Some(Pack {
                r: Some(format!("{}_r", t.name)),
                ..Pack::default()
            });
            t.usage = Usage::Data;
            t.bare = false;
            t.color_space = None;
        }
        (false, Some(_)) => {
            t.pack = None;
            t.fill = None;
        }
        _ => {}
    }
    if let Some(pack) = &mut t.pack {
        for (label, channel) in [
            ("Red", &mut pack.r),
            ("Green", &mut pack.g),
            ("Blue", &mut pack.b),
            ("Alpha", &mut pack.a),
        ] {
            let mut text = channel.clone().unwrap_or_default();
            key_row(
                ui,
                [label, "pack"],
                "",
                channel.is_some(),
                |ui, width| {
                    let edit = egui::TextEdit::singleline(&mut text).hint_text("input or input.g");
                    ui.add_sized(vec2(width, 22.0), edit);
                },
            )
            .then(|| text.clear());
            *channel = (!text.is_empty()).then_some(text);
        }
        help_line(
            ui,
            "Each channel: an input name, its image's red channel; add .g, .b or .a for another.",
        );
        let mut fill = t.fill.unwrap_or([255; 4]);
        let reset = key_row(
            ui,
            ["Fill", "fill"],
            "The channels no input fills, 0–255.",
            t.fill.is_some(),
            |ui, width| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let each = ((width - 12.0) / 4.0).max(24.0);
                for v in &mut fill {
                    ui.add_sized(vec2(each, 22.0), egui::DragValue::new(v).range(0..=255));
                }
            },
        );
        t.fill = if reset {
            None
        } else if t.fill.is_some() || fill != [255; 4] {
            Some(fill)
        } else {
            None
        };
    }
}

/// The Layout view: the GPU block, the resources and their declarations.
pub fn canvas_layout(ui: &mut Ui, edit: &mut ShaderEdit) {
    let checked = match &edit.checked {
        Ok(c) => c,
        Err(e) => {
            let block = match &edit.edit.params {
                None => Ok(Block::default()),
                Some(_) => Err(e.clone()),
            };
            if block.is_err() {
                placeholder(ui, e);
                return;
            }
            ui.label(RichText::new(e).color(CHANNEL_R));
            return;
        }
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                heading(ui, "GPU block");
                let block = &checked.block;
                if block.fields.is_empty() {
                    ui.weak("No GPU block: the shader has no params, or none a GPU can read.");
                } else {
                    egui::Grid::new("layout_block")
                        .striped(true)
                        .spacing(vec2(16.0, 4.0))
                        .show(ui, |ui| {
                            for h in ["Offset", "Size", "Type", "Field"] {
                                ui.label(RichText::new(h).color(TEXT_WEAK));
                            }
                            ui.end_row();
                            for f in &block.fields {
                                let size = match f.ty {
                                    shader::GpuType::Vec3 => 12,
                                    shader::GpuType::Vec4 => 16,
                                    _ => 4,
                                };
                                ui.monospace(f.offset.to_string());
                                ui.monospace(size.to_string());
                                ui.monospace(f.ty.glsl());
                                ui.monospace(&f.path);
                                ui.end_row();
                            }
                        });
                    ui.weak(format!("{} bytes, std140", block.size));
                }
                for path in &block.cpu_only {
                    ui.label(
                        RichText::new(format!("{path}: CPU only"))
                            .monospace()
                            .color(TEXT_WEAK),
                    );
                }
                ui.add_space(12.0);
                heading(ui, "Resources");
                resources_grid(ui, &edit.edit, &checked.resources);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut edit.code, CodeView::Glsl, "GLSL");
                    ui.selectable_value(&mut edit.code, CodeView::Wgsl, "WGSL");
                    let code = match edit.code {
                        CodeView::Glsl => shader::glsl(&edit.edit, block),
                        CodeView::Wgsl => shader::wgsl(&edit.edit, block),
                    };
                    if icon_button(ui, Icon::Files, "Copy", true).clicked() {
                        ui.ctx().copy_text(code);
                    }
                });
                let mut code = match edit.code {
                    CodeView::Glsl => shader::glsl(&edit.edit, block),
                    CodeView::Wgsl => shader::wgsl(&edit.edit, block),
                };
                ui.add(
                    egui::TextEdit::multiline(&mut code)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .interactive(true),
                );
                ui.weak("Sets and bindings here are only a suggestion: assetc finds resources by name.");
            });
        });
}

fn heading(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(TEXT));
    ui.add_space(4.0);
}

fn resources_grid(ui: &mut Ui, shader: &Shader, found: &[Resource]) {
    let at = |kind: shader::ResourceKind, texture: usize| {
        found
            .iter()
            .find(|r| r.kind == kind && r.texture == texture)
            .map_or("unused".to_owned(), |r| {
                let stages = match r.stages {
                    1 => "vertex",
                    2 => "fragment",
                    _ => "both",
                };
                format!("set {} binding {} ({stages})", r.set, r.binding)
            })
    };
    egui::Grid::new("layout_resources")
        .striped(true)
        .spacing(vec2(16.0, 4.0))
        .show(ui, |ui| {
            for h in ["Name", "Kind", "Dimension", "Usage", "In the stages"] {
                ui.label(RichText::new(h).color(TEXT_WEAK));
            }
            ui.end_row();
            use shader::ResourceKind as K;
            for (name, kind) in [("Params", K::Params), ("Frame", K::Frame), ("Object", K::Object)]
            {
                ui.monospace(name);
                ui.monospace("block");
                ui.monospace("");
                ui.monospace("");
                ui.monospace(at(kind, 0));
                ui.end_row();
            }
            for (i, t) in shader.textures.iter().enumerate() {
                ui.monospace(&t.name);
                ui.monospace("texture");
                ui.monospace(t.dimension().key());
                ui.monospace(t.usage.key());
                ui.monospace(at(K::Texture, i));
                ui.end_row();
                ui.monospace(format!("{}_sampler", t.name));
                ui.monospace("sampler");
                ui.monospace("");
                ui.monospace("");
                ui.monospace(at(K::Sampler, i));
                ui.end_row();
            }
        });
}

// Custom materials.

/// A custom material's shader, read with its params' schema.
pub struct MaterialShader {
    /// Relative to the material's folder, as `shader` names it.
    pub path: String,
    pub shader: Shader,
    pub params: Option<(Schema, usize, String)>,
    pub block: Block,
    /// The stages, when there is a fragment one: for the preview.
    pub fragment: Option<Vec<u8>>,
    pub vertex: Option<Vec<u8>>,
    pub resources: Vec<Resource>,
}

/// An `.acmat` being edited.
pub struct CustomEdit {
    pub edit: acmat::Source,
    pub saved: acmat::Source,
    pub file: PathBuf,
    input: PathBuf,
    /// Shaders of the input relative to the material's folder.
    shaders: Vec<String>,
    /// Files of the input relative to the material's folder.
    files: Vec<String>,
    /// Input-relative files for `asset<K>` params, and the instances refs can name.
    assets: Vec<String>,
    refs: Vec<Target>,
    pub shader: Option<Result<MaterialShader, String>>,
    fields: FieldsState,
    /// A shader chosen whose switch drops values, waiting for confirmation.
    switching: Option<(String, Vec<String>)>,
}

/// Reads `path` (relative to `dir`, the material's folder) as the material's shader.
fn read_shader(input: &Path, dir: &Path, path: &str) -> Result<MaterialShader, String> {
    if path.is_empty() {
        return Err("No shader chosen".into());
    }
    let dir_rel = crate::build::relative(input, dir).unwrap_or_default();
    let rel = crate::plan::resolve(&dir_rel, path).ok_or("the shader leaves the input")?;
    let text = std::fs::read_to_string(input.join(&rel)).map_err(|e| format!("{path}: {e}"))?;
    let parsed = Shader::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    // Problems of the stages do not stop editing; the build reports them.
    let loaded = shader::load_parsed(input, &rel, parsed.clone());
    let params = parsed.params.as_ref().and_then(|p| {
        let schema_rel = crate::plan::resolve(crate::plan::split_dir(&rel).0, &p.schema)?;
        let schema = std::fs::read_to_string(input.join(&schema_rel))
            .ok()
            .and_then(|t| Schema::parse(&t).ok())?;
        let si = adata::struct_index(&schema, &p.ty)?;
        Some((schema, si, schema_rel))
    });
    let block = params
        .as_ref()
        .and_then(|(s, si, _)| shader::layout(s, *si).ok())
        .unwrap_or_default();
    let (fragment, vertex, resources) = match loaded {
        Ok(l) => (l.fragment, l.vertex, l.resources),
        Err(_) => (None, None, Vec::new()),
    };
    Ok(MaterialShader {
        path: path.to_owned(),
        shader: parsed,
        params,
        block,
        fragment,
        vertex,
        resources,
    })
}

pub fn prepare_custom(input: &Path, file: &Path) -> Result<CustomEdit, String> {
    let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let source = acmat::Source::parse(&text)?;
    let dir = file.parent().unwrap_or(input);
    let all = crate::build::files_under(input);
    let mut shaders: Vec<String> = all
        .iter()
        .filter(|f| shader::is_shader(f))
        .map(|f| adata::relative(dir, f))
        .collect();
    shaders.sort();
    let mut files: Vec<String> = all.iter().map(|f| adata::relative(dir, f)).collect();
    // Atlas folders fill atlas textures.
    files.extend(
        crate::adata::asset_files(input)
            .into_iter()
            .filter(|f| crate::atlas::is_atlas(Path::new(f)))
            .map(|f| adata::relative(dir, &input.join(f))),
    );
    files.sort();
    let shader = Some(read_shader(input, dir, &source.shader));
    let refs = match &shader {
        Some(Ok(MaterialShader {
            params: Some((_, _, schema_rel)),
            ..
        })) => adata::project(input, &input.join(schema_rel)),
        _ => Vec::new(),
    };
    Ok(CustomEdit {
        edit: source.clone(),
        saved: source,
        file: file.to_path_buf(),
        input: input.to_path_buf(),
        shaders,
        files,
        assets: adata::asset_files(input),
        refs,
        shader,
        fields: FieldsState::default(),
        switching: None,
    })
}

impl CustomEdit {
    pub fn keep(mut self, kept: Option<CustomEdit>) -> Self {
        if let Some(kept) = kept {
            self.fields = kept.fields;
        }
        self
    }

    fn dir(&self) -> &Path {
        self.file.parent().unwrap_or(&self.input)
    }

    /// The shader's texture and input names, in order.
    pub fn order(&self) -> Vec<String> {
        match &self.shader {
            Some(Ok(s)) => s.shader.fillable().into_iter().map(|(n, _)| n).collect(),
            _ => self.edit.textures.iter().map(|(n, _)| n.clone()).collect(),
        }
    }

    /// What fills `name`: its key, else the file found by name.
    pub fn filled(&self, name: &str, usage: Usage, bare: bool) -> Option<(String, bool)> {
        if let Some(t) = self.edit.texture(name) {
            return Some((t.path.clone(), true));
        }
        let stem = self
            .file
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(acmat::SUFFIX))
            .unwrap_or_default();
        self.files
            .iter()
            .filter(|f| !f.contains('/'))
            .find(|f| {
                let s = crate::build::strip_extension(f);
                usage.accepts(Path::new(f))
                    && (s == format!("{stem}.{name}") || (bare && s == stem))
            })
            .map(|f| (f.clone(), false))
    }

    fn switch_to(&mut self, path: String) {
        self.edit.shader = path;
        let shader = read_shader(&self.input, self.dir(), &self.edit.shader);
        if let Ok(s) = &shader {
            let names: Vec<String> = s.shader.fillable().into_iter().map(|(n, _)| n).collect();
            self.edit.textures.retain(|(n, _)| names.contains(n));
            if s.params.is_none() {
                self.edit.params.clear();
            }
            self.refs = match &s.params {
                Some((_, _, rel)) => adata::project(&self.input, &self.input.join(rel)),
                None => Vec::new(),
            };
        }
        self.shader = Some(shader);
    }

    /// The values a switch to `path` would drop.
    fn dropped_by(&self, path: &str) -> Vec<String> {
        let Ok(next) = read_shader(&self.input, self.dir(), path) else {
            return Vec::new();
        };
        let names: Vec<String> = next.shader.fillable().into_iter().map(|(n, _)| n).collect();
        let mut out: Vec<String> = self
            .edit
            .textures
            .iter()
            .filter(|(n, _)| !names.contains(n))
            .map(|(n, _)| format!("texture {n}"))
            .collect();
        let same_params = match (&self.shader, &next.shader.params) {
            (Some(Ok(cur)), Some(p)) => cur.shader.params.as_ref().map(|c| &c.ty) == Some(&p.ty),
            _ => false,
        };
        if !self.edit.params.is_empty() && !same_params {
            out.push("params".into());
        }
        out
    }
}

/// The Material tab of a `.acmat`; returns the file's text when it should be saved.
pub fn custom_tab(ui: &mut Ui, edit: &mut CustomEdit) -> Option<String> {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    property_style(ui);
                    custom_sections(ui, edit);
                });
        });
    commit(ui, edit)
}

fn custom_sections(ui: &mut Ui, edit: &mut CustomEdit) {
    shader_choice(ui, edit);
    let Some(Ok(s)) = &edit.shader else {
        if let Some(Err(e)) = &edit.shader
            && !edit.edit.shader.is_empty()
        {
            ui.label(RichText::new(e).size(12.0).color(CHANNEL_R));
        }
        return;
    };
    if let Some((schema, si, _)) = &s.params {
        let (schema, si) = (schema.clone(), *si);
        let ty = schema.types[si].name.clone();
        section(ui, "Params", |ui| {
            let ids = edit
                .refs
                .iter()
                .map(|t| (t.ty.as_str(), t.id.as_str()))
                .collect();
            let problems: Vec<DataProblem> =
                adata::check_values(&schema, si, &edit.edit.params, &ty, ids, Some(&edit.assets))
                    .into_iter()
                    .map(|(field, message, warning)| DataProblem {
                        instance: 0,
                        field,
                        message,
                        warning,
                    })
                    .collect();
            let problems: Vec<&DataProblem> = problems.iter().collect();
            let mut set = edit.edit.params.clone();
            struct_fields(
                ui,
                &mut edit.fields,
                0,
                &schema,
                si,
                &mut set,
                &problems,
                (&edit.assets, &edit.refs),
            );
            if set != edit.edit.params {
                edit.edit.params = adata::normalise(&schema, si, &set, None);
            }
        });
    }
    let shader = s.shader.clone();
    textures_choice(ui, edit, &shader);
    state_section(ui, &mut edit.edit.state, Some(&shader.state), "acmat");
}

fn commit(ui: &Ui, edit: &mut CustomEdit) -> Option<String> {
    let busy = ui.input(|i| i.pointer.any_down()) || ui.memory(|m| m.focused().is_some());
    (!busy && edit.edit != edit.saved).then(|| {
        edit.saved = edit.edit.clone();
        edit.edit.to_toml(&edit.order())
    })
}

fn shader_choice(ui: &mut Ui, edit: &mut CustomEdit) {
    let mut chosen = None;
    section(ui, "Shader", |ui| {
        let current = if edit.edit.shader.is_empty() {
            RichText::new("Choose a shader").color(TEXT_WEAK)
        } else {
            RichText::new(&edit.edit.shader)
        };
        key_row(
            ui,
            ["Shader", "shader"],
            "The .ashader whose params and textures this material fills.",
            !edit.edit.shader.is_empty(),
            |ui, width| {
                fixed(ui, width, |ui| {
                    egui::ComboBox::from_id_salt("acmat_shader")
                        .width(width)
                        .truncate()
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            if edit.shaders.is_empty() {
                                ui.weak("No shaders in the input");
                            }
                            for s in &edit.shaders {
                                if ui.selectable_label(*s == edit.edit.shader, s).clicked() {
                                    chosen = Some(s.clone());
                                }
                            }
                        });
                });
            },
        )
        .then(|| chosen = Some(String::new()));
        if let Some((path, dropped)) = edit.switching.clone() {
            ui.label(
                RichText::new(format!("{path} drops: {}", dropped.join(", ")))
                    .size(12.0)
                    .color(ACCENT),
            );
            ui.horizontal(|ui| {
                if ui.button("Switch").clicked() {
                    edit.switching = None;
                    edit.switch_to(path);
                }
                if ui.button("Cancel").clicked() {
                    edit.switching = None;
                }
            });
        }
    });
    if let Some(path) = chosen
        && path != edit.edit.shader
    {
        let dropped = edit.dropped_by(&path);
        if dropped.is_empty() || path.is_empty() {
            edit.switch_to(path);
        } else {
            edit.switching = Some((path, dropped));
        }
    }
}

fn textures_choice(ui: &mut Ui, edit: &mut CustomEdit, shader: &Shader) {
    let stem = edit
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(acmat::SUFFIX))
        .unwrap_or_default()
        .to_owned();
    section(ui, "Textures", |ui| {
        help_line(
            ui,
            &format!(
                "Files named {stem}.<name>.<ext> next to the material fill their texture by name. Choose another file to use it instead."
            ),
        );
        for (name, usage) in shader.fillable() {
            let bare = shader.textures.iter().any(|t| t.bare && t.name == name);
            let packed_in = shader.textures.iter().find(|t| {
                t.pack.as_ref().is_some_and(|p| {
                    p.channels()
                        .into_iter()
                        .flatten()
                        .any(|c| shader::pack_input(c).is_ok_and(|(n, _)| n == name))
                })
            });
            let label = match packed_in {
                Some(t) => format!("{name} ({})", t.name),
                None => name.clone(),
            };
            let filled = edit.filled(&name, usage, bare);
            let set = edit.edit.texture(&name).is_some();
            let text = match &filled {
                Some((path, true)) => RichText::new(path),
                Some((path, false)) => RichText::new(format!("{path} (by name)")).color(TEXT_WEAK),
                None => RichText::new("None").color(TEXT_WEAK),
            };
            let by_name = if set { None } else { filled.clone() };
            let mut choice: Option<Option<String>> = None;
            let hint = format!("{name}, by name: {stem}.{name}.<ext>");
            let reset = key_row(ui, [&label, &hint], "", set, |ui, width| {
                fixed(ui, width, |ui| {
                    egui::ComboBox::from_id_salt(("acmat_texture", &name))
                        .width(width)
                        .truncate()
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            let by = by_name
                                .as_ref()
                                .map_or("By name: none".to_owned(), |(p, _)| format!("By name: {p}"));
                            if ui.selectable_label(!set, by).clicked() {
                                choice = Some(None);
                            }
                            for f in edit.files.iter().filter(|f| usage.accepts(Path::new(f))) {
                                let on = edit.edit.texture(&name).is_some_and(|t| t.path == *f);
                                if ui.selectable_label(on, f).clicked() {
                                    choice = Some(Some(f.clone()));
                                }
                            }
                        });
                });
            });
            if reset {
                choice = Some(None);
            }
            if usage == Usage::Normal
                && let Some(t) = edit.edit.textures.iter_mut().find(|(n, _)| *n == name)
            {
                let mut dx = t.1.normal_y == Some(NormalY::Dx);
                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() * 0.4);
                    if ui.checkbox(&mut dx, "DirectX normal map (+Y down)").changed() {
                        t.1.normal_y = dx.then_some(NormalY::Dx);
                    }
                });
            }
            match choice {
                Some(None) => edit.edit.textures.retain(|(n, _)| *n != name),
                Some(Some(path)) => {
                    match edit.edit.textures.iter_mut().find(|(n, _)| *n == name) {
                        Some(t) => t.1.path = path,
                        None => edit.edit.textures.push((
                            name.clone(),
                            TextureRef {
                                path,
                                normal_y: None,
                            },
                        )),
                    }
                }
                None => {}
            }
        }
    });
}

/// The Material view of a custom material without a preview: its GPU block and textures.
pub fn canvas_custom(ui: &mut Ui, edit: &CustomEdit) {
    let Some(Ok(s)) = &edit.shader else {
        let text = match &edit.shader {
            Some(Err(e)) => e.clone(),
            _ => "No shader chosen".into(),
        };
        placeholder(ui, &text);
        return;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            Frame::new().inner_margin(Margin::same(16)).show(ui, |ui| {
                if s.fragment.is_none() {
                    ui.weak(format!(
                        "No preview: {} has no fragment stage.",
                        s.path
                    ));
                    ui.add_space(8.0);
                }
                heading(ui, "GPU block");
                let values = match &s.params {
                    Some((schema, si, _)) => adata::resolve(schema, *si, &edit.edit.params, None),
                    None => Map::new(),
                };
                let bytes = shader::pack(&s.block, &values);
                if s.block.fields.is_empty() {
                    ui.weak("No GPU block.");
                }
                egui::Grid::new("custom_block")
                    .striped(true)
                    .spacing(vec2(16.0, 4.0))
                    .show(ui, |ui| {
                        for f in &s.block.fields {
                            ui.monospace(f.offset.to_string());
                            ui.monospace(&f.path);
                            ui.monospace(gpu_value(f, &bytes));
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
                heading(ui, "Textures");
                egui::Grid::new("custom_textures")
                    .striped(true)
                    .spacing(vec2(16.0, 4.0))
                    .show(ui, |ui| {
                        for t in &s.shader.textures {
                            ui.monospace(&t.name);
                            let what = match &t.pack {
                                None => edit
                                    .filled(&t.name, t.usage, t.bare)
                                    .map(|(p, _)| p),
                                Some(pack) => {
                                    let parts: Vec<String> = pack
                                        .channels()
                                        .into_iter()
                                        .flatten()
                                        .filter_map(|c| {
                                            let (n, _) = shader::pack_input(c).ok()?;
                                            let (p, _) = edit.filled(n, Usage::Data, false)?;
                                            Some(format!("{n}: {p}"))
                                        })
                                        .collect();
                                    (!parts.is_empty()).then(|| parts.join(", "))
                                }
                            };
                            match what {
                                Some(p) => ui.monospace(p),
                                None => ui.label(
                                    RichText::new(format!("empty, binds {}", t.fallback().key()))
                                        .color(TEXT_WEAK),
                                ),
                            };
                            ui.end_row();
                        }
                    });
            });
        });
}

/// A GPU value from the block's bytes, as text.
fn gpu_value(f: &shader::GpuField, bytes: &[u8]) -> String {
    let at = |o: u32| {
        bytes
            .get(o as usize..o as usize + 4)
            .map_or([0; 4], |b| b.try_into().unwrap())
    };
    let float = |o| f32::from_le_bytes(at(o));
    let floats = |n: u32| {
        (0..n)
            .map(|i| format!("{:.3}", float(f.offset + i * 4)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match f.ty {
        shader::GpuType::F32 => format!("{}", float(f.offset)),
        shader::GpuType::I32 => i32::from_le_bytes(at(f.offset)).to_string(),
        shader::GpuType::U32 => {
            let v = u32::from_le_bytes(at(f.offset));
            match f.values.get(v as usize) {
                Some(name) => format!("{v} ({name})"),
                None => v.to_string(),
            }
        }
        shader::GpuType::Vec3 => format!("{} (linear)", floats(3)),
        shader::GpuType::Vec4 => format!("{} (linear)", floats(4)),
    }
}
