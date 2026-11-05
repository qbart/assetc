# `<name>.amat`

Optional material definition next to a material's textures; see
[`docs/features/materials.md`](../features/materials.md) for how materials are
found and built. assetc reads it, and its Material tab writes it (see
[Written by assetc](#written-by-assetc)).

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8. File name `<name>.amat`,
where `<name>` is the material name (case-sensitive). The extension is
lowercase `.amat`.

Materials used to be `<name>.mat.toml`; such files are rejected (see
[`materials.md`](../features/materials.md#old-mattoml-files)). The contents
are the same.

## Schema

Every key is optional; an empty file is a valid material that uses the
defaults. Unknown keys are rejected; `[[structs]]` and `[values]` hold
[attached structs](#structs). With `shader` the file is a
[custom material](#custom-materials) and has other keys.

| Key                  | Type               | Range / values                  | Default (see materials.md) |
|----------------------|--------------------|---------------------------------|----------------------------|
| `base_color`         | array of 4 floats  | each `0..=1`, linear RGBA        | `[1, 1, 1, 1]`             |
| `metallic`           | float              | `0..=1`                         | `1.0` with a map, else `0.0` |
| `roughness`          | float              | `0..=1`                         | `1.0`                      |
| `occlusion_strength` | float              | `0..=1`                         | `1.0`                      |
| `normal_scale`       | float              | any finite                      | `1.0`                      |
| `normal_y`           | string             | `"gl"` (+Y up), `"dx"` (+Y down) | `"gl"`                    |
| `emissive`           | array of 3 floats  | each `0..=1`, linear RGB         | `[0, 0, 0]`, or `[1, 1, 1]` with a map |
| `emissive_strength`  | float              | `>= 0`                          | `1.0`                      |
| `height_mode`        | string             | `"parallax"`, `"displacement"`  | `"parallax"`               |
| `height_scale`       | float              | `>= 0`                          | `0.05`                     |
| `height_midlevel`    | float              | `0..=1`                         | `0.5`                      |
| `alpha_mode`         | string             | `"opaque"`, `"mask"`, `"blend"` | `"opaque"`                 |
| `alpha_cutoff`       | float              | `0..=1`, used by `"mask"`       | `0.5`                      |
| `double_sided`       | bool               |                                 | `false`                    |
| `[textures]`         | table              | see below                       | empty                      |

All floats must be finite. A value out of range fails the material. Integers
are accepted where floats are expected (`metallic = 1`).

### `[textures]`

Keys are slot names: `base_color`, `normal`, `roughness`, `metallic`,
`occlusion`, `emissive`, `height`. Values are paths to texture sources:

- relative to the directory holding the `.amat`, `/`-separated;
- `.` and `..` are allowed but the result must stay inside `input`;
- the file must exist and be a texture source (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`).

A key here replaces the convention file for that slot.

## Structs

[Attached structs](../features/structs.md) of the material:

- **`[[structs]]`**: one table per attachment, in order: `schema` (the
  schema's path relative to this file's folder, forward slashes) and `type`
  (the struct's name), both strings; any other key is an error.
- **`[values.<Type>]`**: the material's values of the attached struct `<Type>`,
  one key per field it sets, sparse and normalised as an
  [instance's](adata.md#written-form), each value in
  [TOML form](../features/structs.md#values-in-toml) (struct values and
  lists inline, `null` as `{ "$null" = true }`). A struct with nothing set
  has no table.

assetc writes the `[[structs]]` tables, then the `[values.<Type>]` tables in
attachment order (struct values inline), each after a blank line:

```toml
[[structs]]
schema = "game.aschema"
type = "Pickup"

[values.Pickup]
mass = 2.5
tags = ["loot", "rare"]
stats = { hp = 40, armor = 2 }
```

## Custom materials

> **Status: design**, see [`custom-materials.md`](../features/custom-materials.md).

A `.amat` with `shader` is a custom material. Its keys:

| Key | Type | Meaning |
|-----|------|---------|
| `shader` | string | Path of a [`.ashader`](ashader.md), relative to this file's folder, inside `input` |
| `blend`, `alpha_cutoff`, `cull`, `depth_write`, `depth_test` | as in [`[state]`](ashader.md#state) | Override the shader's state |
| `[params]` | table | The params struct's values, sparse and normalised, in [TOML form](../features/structs.md#values-in-toml) (struct values inline) |
| `[textures]` | table | One key per binding or input name of the shader: a path (relative to this file's folder, inside `input`) or `{ path = "…", normal_y = "gl" \| "dx" }` (`normal_y` only for `normal` bindings) |
| `[[structs]]`, `[values.<Type>]` | | [Attached structs](#structs), as for PBR materials |

The PBR keys (`base_color` … `double_sided`) are errors in a custom
material, and a key in `[textures]` that is not one of the shader's names is
an error. The Material tab writes `shader`, the state keys in the order of
the table above, `[params]`, `[textures]` (keys in the shader's binding and
input order), then the structs, under the rules of
[Written by assetc](#written-by-assetc).

```toml
shader = "../shaders/water.ashader"
blend = "blend"

[params]
depth_color = "#0b3d4f"
wave_speed = 1.5
foam = { amount = 0.4, scale = 8.0 }

[textures]
color = "lake.png"
ripples = { path = "../shared/ripples.n.png", normal_y = "dx" }
```

## Written by assetc

The Material tab (see
[`materials.md`](../features/materials.md#material-tab)) rewrites the whole
file:

- only the keys that are set, in the order of the [schema](#schema) table,
  then `[textures]` (only when it has a key) with its keys in slot order
  (`base_color`, `normal`, `roughness`, `metallic`, `occlusion`, `emissive`,
  `height`), then the [structs](#structs);
- floats rounded to 4 decimals and written in their shortest form with a
  decimal point (`0.5`, `1.0`);
- strings in double quotes; arrays inline (`[0.8, 0.8, 0.8, 1.0]`);
- comments and blank lines of the old file are not kept; the file ends with a
  newline. A material with no keys set is written as an empty file.

## Example

```toml
# samples/brick/brick.amat
roughness = 0.9
height_mode = "parallax"
height_scale = 0.03

[textures]
occlusion = "brick.ao.jpg"
```
