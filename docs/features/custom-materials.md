# Custom materials

> **Status: in progress.** The build is done; the GUI, the
> [preview](#preview) and the [SDK](#sdk) are not yet. Formats: [`.ashader`](../spec/ashader.md),
> [`.ashader.bin`](../spec/ashader.bin.md),
> [`.acmat`](../spec/acmat.md), [`.acmat.bin`](../spec/acmat.bin.md).

[Materials](materials.md) are glTF metallic-roughness PBR: fixed values and
fixed texture slots. A **custom material** is made for a shader assetc does not
know: its values are any data and its textures any textures, both declared by
a **shader** file. assetc checks, edits and compiles what the shader
declares, without knowing what the shader does with it. It can preview the
material only when the shader comes with SPIR-V code (see
[Preview](#preview)).

```
shaders/water.ashader      what a water material has: params, textures, state, code
shaders/water.frag.spv     optional SPIR-V, for the preview and the game
data/game.aschema          struct WaterParams: the params' fields
water/lake.acmat           shader = "../shaders/water.ashader", its params and textures
water/lake.foam.png        fills the shader's `foam` input by name
```

## Shaders

A `*.ashader` file (TOML, see [`ashader.md`](../spec/ashader.md)) declares a
shader's interface:

- **Params**: the material's values, as one [schema](schemas.md) struct
  (`params = { schema = "…", type = "WaterParams" }`). Fields, types,
  defaults, ranges, enums, groups and conditions all come from the schema, so
  params are edited and checked like [instances](instances.md). A shader
  without params is valid.
- **Textures**: the shader's textures, in order, each with a
  name, a usage, a dimension, a color space and a fallback. A texture is
  filled by one input file, or **packed** from channels of several inputs
  (ORM generalised, see [Packing](#packing)).
- **State**: blending, culling and depth, which every material of the shader
  shares unless it overrides them.
- **Stages**: optional SPIR-V files (`*.spv`) for the fragment and vertex
  stages.

```toml
# shaders/water.ashader
doc = "Lake and river water"
params = { schema = "../data/game.aschema", type = "WaterParams" }

[state]
blend = "blend"
cull = "none"

[stages]
fragment = "water.frag.spv"

[[textures]]
name = "color"
usage = "color"
bare = true                  # water/lake.png fills it by name

[[textures]]
name = "ripples"
usage = "normal"
default = "normal"

[[textures]]
name = "mask"
usage = "data"
pack = { r = "foam", g = "depth", b = "caustics" }

[[textures]]
name = "sky"
usage = "environment"
dimension = "cube"
```

### Params

The params struct is compiled twice, both from the same values:

- **GPU block**: the fields a shader can read, laid out by **std140** rules
  in field order (the schema's `fields` order, not the editor's `order`), so
  it can be uploaded as a uniform buffer as-is and declared the same way in
  GLSL (`layout(std140)`) and WGSL (`var<uniform>`).
- **Values**: every field, resolved, in the
  [struct chunks](../spec/astruct.md) (scope `3`), for the game's CPU side.

| Field type | In the GPU block | std140 align / size |
|------------|------------------|---------------------|
| `bool` | `u32`, `0` or `1` | 4 / 4 |
| `i8`, `i16`, `i32` | `i32` | 4 / 4 |
| `u8`, `u16`, `u32` | `u32` | 4 / 4 |
| `f32`, `f64` | `f32` (`f64` rounded) | 4 / 4 |
| enum | `u32`, the value's index | 4 / 4 |
| `rgb` | `vec3<f32>`, linear | 16 / 12 |
| `rgba` | `vec4<f32>`, linear, alpha as is | 16 / 16 |
| struct `S` | `S`'s GPU fields as a nested struct | 16 / rounded up to 16 |
| `i64`, `u64`, `string`, `curve`, `gradient`, `list<T>`, `ref<S>`, `asset<K>` | not in the block (CPU only) | — |

- An optional field set to `null` holds its type's implicit default in the
  block; the values keep the `null`.
- A nested struct with no GPU fields is left out of the block.
- The block's size is rounded up to 16 bytes. A shader whose params have no
  GPU fields has no block. A block over **16 KiB** (the smallest uniform
  buffer limit in Vulkan, WebGPU and GL ES) fails the shader.
- A vector is a struct whose fields happen to be floats: a
  `Vec2 { x: f32, y: f32 }` field is a nested struct of 16 bytes, not a
  `vec2`. Use `rgb` / `rgba` for colours.

The Shader tab and the Layout view show the block (see [GUI](#gui)) and
give it as GLSL and WGSL declarations to paste into the shader.

### Textures

Each `[[textures]]` entry is one texture the shader samples:

| Key | Meaning |
|-----|---------|
| `name` | Identifier (`[a-z][a-z0-9_]*`), unique in the shader. The `.acmat`'s key for it and, unpacked, its by-name suffix. |
| `usage` | What it holds; sets the default color space and which files can fill it (below). Written to the KTX2 [usage key](../spec/ktx2.md). |
| `dimension` | `2d` (default), `3d`, `cube`. A file that compiles to another dimension fails the material. |
| `color_space` | `srgb` or `linear`; the default comes from `usage`. Only for image files: other assets keep their own. |
| `default` | What the game binds when a material leaves it empty: `white` (default), `black`, `gray`, `normal` (`0.5, 0.5, 1`), `transparent`. |
| `required` | `true`: a material without it fails. Default `false`. |
| `bare` | `true`: the material's unsuffixed file (`lake.png`) fills it by name, as PBR's base color. At most one texture, unpacked. |
| `mipmaps` | `false`: compiled without mipmaps, whatever `no_mipmaps` says. Default `true` (then `no_mipmaps` applies). |
| `pack` | Builds the texture from other inputs' channels instead, see [Packing](#packing). |
| `fill` | With `pack`: the value (0–255) of each channel no input fills, `[255, 255, 255, 255]` by default. |

| `usage` | Filled by | Color space |
|---------|-----------|-------------|
| `color` | an image (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`) | sRGB |
| `data`, `height` | an image | linear |
| `normal` | an image; `normal_y = "dx"` on the material's key flips green, as for PBR | linear |
| `noise` | a [noise](noise.md) (`*.anoise`) | its own |
| `gradient` | a [gradient file](gradients.md) (`*.agradient`) | its own |
| `palette` | a [palette](palettes.md) (`*.apalette`) | its own |
| `lut` | a [color LUT](lut.md) (`*.cube`); `dimension = "3d"` needs it in `volume_luts` | its own |
| `environment` | an [HDR environment](hdr.md) (`*.exr`), `dimension = "cube"` | its own |
| `atlas` | an [atlas](atlases.md) (`*.atlas` folder) | its own |

An image is compiled with the texture's settings, like a PBR slot texture
(the same file used with conflicting color spaces, `normal_y` or mipmaps by
any materials fails the build). The other assets compile as they always do,
and the material refers to their texture.

### Packing

`pack` maps the channels of the texture to **inputs**, each an input name and
optionally the input's channel (`foam`, `foam.r`, `depth.a`; `.r` by
default):

```toml
[[textures]]
name = "mask"
usage = "data"
pack = { r = "foam", g = "depth", b = "caustics.g" }
fill = [0, 255, 0, 255]
```

- An input is filled like an unpacked texture: by the material's key of its
  name, or by name (`lake.foam.png`). It makes no texture of its own.
- Inputs are images. All inputs of one material's packed texture must have the
  same size; a channel whose input the material leaves empty gets `fill`.
- The packed texture is linear. Its asset path is
  `<dir>/<material>.<texture>` (`water/lake.mask`), as `.orm` is for PBR.
- A material with none of a pack's inputs gets no texture there
  (`0`, the game binds `default`).
- Input names share the texture names' namespace: an input may not be named
  like a texture.

### State

| Key | Values | Default |
|-----|--------|---------|
| `blend` | `opaque`, `mask`, `blend`, `add`, `multiply` | `opaque` |
| `alpha_cutoff` | `0..=1`, for `mask` | `0.5` |
| `cull` | `back`, `front`, `none` | `back` |
| `depth_write` | bool | `true` (`false` for `blend`, `add`, `multiply`) |
| `depth_test` | `less`, `less_equal`, `equal`, `always` | `less_equal` |

A material may set any of them to override its shader's.

### Stages

`[stages]` names SPIR-V binaries (`*.spv`, relative to the shader's folder,
inside `input`): `fragment` and `vertex`, each optional, with the entry
point `entry` (`"main"` by default, per stage: `fragment_entry`,
`vertex_entry`).

- assetc does not compile shader sources; build the `.spv` with your own
  tools (`glslc`, `glslangValidator`, `naga`, `slangc`, `dxc -spirv`).
- The build parses each stage, checks it against the shader's interface
  (see [The interface in SPIR-V](#the-interface-in-spir-v)) and puts it in the
  `.ashader.bin`, so the game can create the pipeline from it.
- A shader without stages is fine: the game brings its own code and uses the
  `.ashader.bin` for the layout. Such materials are not previewed.

### The interface in SPIR-V

assetc finds a stage's resources **by name**, at whatever descriptor set and
binding the stage declares them. It asks for no particular numbers, so a
shader keeps the layout its game's renderer uses; the `.ashader.bin` records
the numbers it found (see [`ashader.bin.md`](../spec/ashader.bin.md#bindings)).
Names come from the SPIR-V's debug names (`OpName`), which every compiler
writes by default; a stage stripped of them (`spirv-opt --strip-debug`)
fails the shader with `<file>: has no names; compile without stripping debug
info`.

| Name (variable or block type) | Resource | Notes |
|-------------------------------|----------|-------|
| `Params` / `params` | uniform block | The GPU block; each member it declares must have the block's offset and type for that field, by name (nested structs by their members' names). It may declare fewer members. |
| a texture's `name` | texture | `texture2D`, `texture3D` or `textureCube` by the texture's `dimension` |
| `<name>_sampler` | sampler | The texture's sampler; optional |
| `Frame` / `frame` | uniform block | Optional, filled by the preview, see below |
| `Object` / `object` | uniform block | Optional, filled by the preview: `mat4 model`, `mat4 normal` (inverse transpose of `model`) |

```glsl
// The numbers are yours; only the names matter.
layout(set = 1, binding = 0, std140) uniform Params {
    vec3 depth_color;
    float wave_speed;
} params;
layout(set = 1, binding = 1) uniform texture2D color;
layout(set = 1, binding = 2) uniform sampler color_sampler;
layout(set = 0, binding = 0, std140) uniform Frame {
    mat4 view_proj;
    vec4 time;
} frame;
```

`Frame`'s members, any of them, in any order:

| Member | Type | Value in the preview |
|--------|------|----------------------|
| `view_proj` | `mat4` | view-projection |
| `view` | `mat4` | view |
| `camera_position` | `vec4` | xyz world, w `1` |
| `light_direction` | `vec4` | xyz world, towards the key light; w `0` |
| `light_color` | `vec4` | rgb linear, w intensity |
| `time` | `vec4` | x seconds since the preview started, y frame delta |
| `viewport` | `vec4` | width, height, 1 / width, 1 / height |

- Textures and samplers are separate (`texture2D` + `sampler`, used as
  `sampler2D(color, color_sampler)`); combined image samplers are rejected.
- **Fragment input** without a `vertex` stage, from assetc's own vertex
  stage: location 0 `vec3` world position, 1 `vec3` world normal, 2 `vec2`
  UV 0, 3 `vec4` world tangent (w = handedness), 4 `vec4` vertex color.
- **Vertex input**, for a `vertex` stage: location 0 `vec3` position,
  1 `vec3` normal, 2 `vec2` UV 0, 3 `vec4` tangent, 4 `vec4` color
  (`1, 1, 1, 1` when the mesh has none). Its output must give the fragment
  stage what it reads.
- **Fragment output**: location 0 `vec4`, linear color (the preview
  tone-maps it); alpha is used by the shader's `blend`.
- The build fails the shader when a stage's `Params` member has another
  offset or type than the block's field of that name, or a name the block
  has not; a texture has another dimension; a resource name is used at two
  different set / binding pairs across the stages; two resources share a set
  and binding; or the entry point is missing or of the wrong stage. Uniform
  blocks, textures and samplers with other names are reported as
  `<file>: <name> is not part of the interface` and fail it too.

## Materials

A custom material is a `<name>.acmat` file (TOML, see
[`acmat.md`](../spec/acmat.md)) naming its shader, relative to its folder:

```toml
# water/lake.acmat
shader = "../shaders/water.ashader"
blend = "blend"                    # overrides the shader's state

[params]
depth_color = "#0b3d4f"
wave_speed = 1.5
foam = { amount = 0.4, scale = 8.0 }

[textures]
color = "lake.png"
sky = "../sky.exr"
ripples = { path = "../shared/ripples.n.png", normal_y = "dx" }
```

- **`[params]`**: the params struct's values, sparse and normalised like an
  [attached struct's](structs.md) ([TOML form](structs.md#values-in-toml)).
  Checked like instances: a type mismatch or unknown enum value fails the
  material; out-of-range numbers, unknown keys and refs or assets that do not
  resolve are warnings ([build marks](gui.md#build-marks) turn it orange).
- **`[textures]`**: one key per texture or input name, a path (relative to
  the `.acmat`'s folder) or `{ path, normal_y }`.
- **By name**: files next to the material named `<material>.<name>.<ext>`
  fill the texture or input `<name>`, and `<material>.<ext>` fills the `bare`
  texture. A key in `[textures]` wins. Only the shader's names count; the PBR
  suffixes (`.n`, `.r`, …) mean nothing here.
- Where `<name>.acmat` exists, the folder's `<name>.*` files belong to it and
  no PBR material `<name>` is formed from them; a `<name>.amat` next to it is
  an error.
- **Structs** can be attached as to PBR materials (`[[structs]]`,
  `[values.<Type>]`); `asset<material>` fields take `*.amat` and `*.acmat`.

Models' glTF materials stay PBR; mapping them to custom materials is
[not designed yet](#later).

## Build

- **Shader** `<dir>/<name>.ashader` → `<dir>/<name>.ashader.bin`
  ([spec](../spec/ashader.bin.md)), manifest kind `13` (`shader`): the
  interface, the GPU block layout, the resources found in the stages and the
  SPIR-V. Built whether a material uses it or not.
- **Custom material** `<dir>/<name>.acmat` → `<dir>/<name>.acmat.bin`
  ([spec](../spec/acmat.bin.md)), manifest kind `14` (`custom material`): the
  shader's asset ID, the state, the GPU block bytes ready to upload, the
  texture asset IDs in the shader's texture order (`0` = empty, bind the
  default), and the params and attached structs as struct chunks.
- A material is rebuilt when its `.acmat`, shader, params schema, `.spv`
  files or any texture input changes.

The build fails the shader, and every material using it, when the
`.ashader` does not parse, its params struct cannot be found, a texture is
invalid, the block is over 16 KiB, or a stage does not parse or does not
match the interface. A material fails on its own when its shader fails,
`[textures]` points at a missing file or one its texture cannot take, a
`required` texture is empty, pack inputs differ in size, or a params value
has an error.

```
shaders/water.ashader -> shaders/water.ashader.bin (shader: 64-byte params, 4 textures, fragment)
water/lake.png -> water/lake.ktx2 (srgb, 11 levels)
water/lake.foam.png, water/lake.depth.png -> water/lake.mask.ktx2 (linear, 11 levels)
water/lake.acmat -> water/lake.acmat.bin (custom material: water, 3 textures)
```

## GUI

### Shader files

- **New Shader** in the Input tree's context menu creates `shader.ashader`
  (`[[textures]]` `color`, usage `color`, `bare`; no params) and starts
  renaming it. `*.ashader` rows use the icon `sparkles`; `*.spv` rows
  `binary` (both new Lucide icons).
- **Info** tab: Params (`WaterParams (game.aschema)` or `none`), GPU block
  (`64 bytes` or `none`), Textures (count), Stages (`fragment`, `vertex`,
  `none`), Asset path, Output, Error.
- **Shader** tab (icon `sparkles`), in the property editor style:
  - *Params*: the schema and struct, picked as in the
    [Structs tab](structs.md#structs-tab)'s Attach menu; a Clear button.
  - *Textures*: one card per texture, in order, with its keys as rows; add,
    remove and drag to reorder (the order of the material's texture IDs). Pack channels are four drop-downs of the input names
    plus *New input…*.
  - *State*: the [State](#state) keys.
  - *Stages*: two drop-downs of the `*.spv` files in `input`, plus `None`.
  - Saved like the other editors, whole file, keys in spec order.
- **Canvas — Layout view**: the GPU block as a table (offset, size, type,
  field path; CPU-only fields listed after it in `TEXT_WEAK` with
  `CPU only`), the resources table (name, kind, dimension, usage, and the
  set / binding each stage declares it at, or `unused`), and **GLSL** /
  **WGSL** tabs with the interface declarations (at sets and bindings of
  assetc's choosing, to be changed freely), each with a **Copy** button. Problems (a stage that does not match) show
  under the tables in `CHANNEL_R`, one line each.
- **`*.spv`** files: the Info tab shows SPIR-V version, generator, size,
  entry points with their stages, and every resource binding (set, binding,
  type, name); the canvas shows the same as a table.

### Custom materials

- **New Material ▸ Custom Material** in the Input tree's context menu
  creates `material.acmat` with `shader = ""`, selects it and starts renaming
  it. `*.acmat` rows use the icon `gem`, as `*.amat`.
- The **Material** tab of a `.acmat` starts with a **Shader** row: a
  drop-down of every `*.ashader` in `input`. Choosing another shader drops the
  params and textures it has no use for; when any are set, a confirmation
  lists them first. Without a shader the tab shows only this row.
- With a shader, the sections are:
  - *Params*: the params struct's fields with the instance editors
    (groups, conditions, defaults shown `TEXT_WEAK`, reset per field), as in
    the [Structs tab](structs.md#structs-tab).
  - *Textures*: one row per texture (packed textures: one row per input,
    under the texture's name), drop-downs as for PBR slots, listing only the
    files the texture's usage can take; `normal` rows have the `normal_y`
    toggle.
  - *State*: the shader's state with its values shown as defaults.
- The Info tab adds Shader and Problems rows.

## Preview

- **With a `fragment` stage**: the canvas shows the material on the
  [preview shape](materials.md#3d-preview) (same toolbar, lighting choice,
  rotation and controls), drawn with the shader's SPIR-V, the edited params
  (before saving) and the textures as the build would make them (scaled to at
  most 2048 px). The frame's light is the preview's key light; an HDR
  environment shows as the background but is not given to the shader (bind
  one in a `cube` texture to use it). `time` runs while the canvas is shown.
- **Without**: no 3D view. The canvas shows the **Material view**: the GPU
  block as a table (offset, field, value) and the bound textures as
  thumbnails with their names, empty textures showing their default.
- A stage that fails to load or to create a pipeline shows its error in
  `CHANNEL_R` in the canvas' bottom-left corner instead of the shape;
  editing values keeps working. Saving a new `.spv` reloads the preview.

## SDK

`Kind::Shader` and `Kind::CustomMaterial`; `Shader` gives the params type,
the block layout (fields by path: offset, type), the textures (usage,
dimension, default, packing), the state, the bindings the stages declare
(set, binding, stages, by name or by kind) and the SPIR-V words with their
entry points; `CustomMaterial` gives the shader ID, state, the block bytes,
texture IDs and the params as `Structs` (`Scope::Params`, scope 3) next to
its attached structs. See [`sdk.md`](sdk.md#custom-materials).

## Later

- Mapping a model's glTF materials to custom materials in its `.amod`.
- `curve` and `gradient` params baked into textures the shader can sample.
- Shader sources (GLSL, WGSL, Slang) compiled by assetc.
- Several passes, specialization constants and keyword variants.
- Environment lighting (irradiance, specular) given to custom shaders.
- Textures with fewer channels (`R8`, `R8G8`) for data that needs fewer than four.
