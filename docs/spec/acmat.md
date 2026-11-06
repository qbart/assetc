# `<name>.acmat`

> **Status: design**, see [`custom-materials.md`](../features/custom-materials.md).

A [custom material](../features/custom-materials.md): the values and
textures of one material for a [shader](ashader.md). assetc reads it, and
its Material tab writes it.

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8. File name `<name>.acmat`,
where `<name>` is the material name (case-sensitive); the extension is
lowercase. Files named `<name>.<texture>.<ext>` next to it fill its shader's
textures by name (see
[Materials](../features/custom-materials.md#materials)).

## Schema

Unknown keys are errors.

| Key | Type | Meaning |
|-----|------|---------|
| `shader` | string | Path of a [`.ashader`](ashader.md), relative to this file's folder, inside `input`. Required; `""` (a new file) builds nothing and fails with `no shader chosen` |
| `blend`, `alpha_cutoff`, `cull`, `depth_write`, `depth_test` | as in [`[state]`](ashader.md#state) | Override the shader's state |
| `[params]` | table | The params struct's values, sparse and normalised, in [TOML form](../features/structs.md#values-in-toml) (struct values inline) |
| `[textures]` | table | One key per texture or input name of the shader: a path (relative to this file's folder, inside `input`) or `{ path = "…", normal_y = "gl" \| "dx" }` (`normal_y` only for `normal` textures and their inputs) |
| `[[structs]]`, `[values.<Type>]` | | [Attached structs](amat.md#structs), as in `.amat` |

A key in `[textures]` that is not one of the shader's texture or input names
is an error.

## Written by assetc

The Material tab rewrites the whole file: `shader`, the state keys in the
order of the table above (only those set), `[params]` (only when it has a
key), `[textures]` (keys in the shader's texture and input order), then the
structs, as [`.amat`](amat.md#written-by-assetc) writes them: floats
rounded to 4 decimals with a decimal point, strings in double quotes, inline
arrays and tables, a blank line between tables, comments not kept, a
trailing newline.

## Example

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
