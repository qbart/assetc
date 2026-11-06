# `<name>.ashader`

> **Status: design**, see [`custom-materials.md`](../features/custom-materials.md).

The interface of a shader that [custom materials](../features/custom-materials.md)
are made for: params, textures, render state and optional SPIR-V
stages. assetc reads it, and its Shader tab writes it (see
[Written by assetc](#written-by-assetc)).

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8. File name `<name>.ashader`;
the extension is lowercase. The shader's name is `<name>`.

## Schema

Unknown keys are errors, at every level.

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `doc` | string | `""` | One-line description, shown in the Inspector |
| `params` | table `{ schema, type }` | none | The params struct: `schema` is the `.aschema`'s path relative to this file's folder (forward slashes), `type` the name of a struct in it |
| `[state]` | table | see below | Render state |
| `[stages]` | table | none | SPIR-V stages |
| `[[textures]]` | array of tables | none | Textures, in order |

### `[state]`

| Key | Type | Values | Default |
|-----|------|--------|---------|
| `blend` | string | `"opaque"`, `"mask"`, `"blend"`, `"add"`, `"multiply"` | `"opaque"` |
| `alpha_cutoff` | float | `0..=1` | `0.5` |
| `cull` | string | `"back"`, `"front"`, `"none"` | `"back"` |
| `depth_write` | bool | | `true`; `false` when `blend` is `blend`, `add` or `multiply` |
| `depth_test` | string | `"less"`, `"less_equal"`, `"equal"`, `"always"` | `"less_equal"` |

### `[stages]`

| Key | Type | Default | Meaning |
|-----|------|---------|---------|
| `fragment` | string | none | Path of a `*.spv` file, relative to this file's folder, inside `input` |
| `fragment_entry` | string | `"main"` | Its entry point; must be a fragment entry point |
| `vertex` | string | none | Path of a `*.spv` file |
| `vertex_entry` | string | `"main"` | Its entry point; must be a vertex entry point |

`fragment` and `vertex` may name the same file.

### `[[textures]]`

| Key | Type | Default | Values |
|-----|------|---------|--------|
| `name` | string | required | `[a-z][a-z0-9_]*`, at most 32 bytes, unique among textures and inputs |
| `usage` | string | required | `"color"`, `"data"`, `"normal"`, `"height"`, `"noise"`, `"gradient"`, `"palette"`, `"lut"`, `"environment"`, `"atlas"` |
| `dimension` | string | `"2d"` (`"cube"` for `environment`) | `"2d"`, `"3d"`, `"cube"` |
| `color_space` | string | `"srgb"` for `color`, else `"linear"` | `"srgb"`, `"linear"`; only with `color`, `data`, `normal`, `height` |
| `default` | string | `"white"` (`"normal"` for `normal`) | `"white"`, `"black"`, `"gray"`, `"normal"`, `"transparent"` |
| `required` | bool | `false` | |
| `bare` | bool | `false` | at most one texture; not with `pack` |
| `mipmaps` | bool | `true` | |
| `channels` | string | `"rgba"` | `"rgba"`, `"rg"`, `"r"`; `"srgb"` needs `"rgba"` |
| `pack` | table | none | keys `r`, `g`, `b`, `a` (at least one), each `"<input>"` or `"<input>.<r\|g\|b\|a>"`; only with usage `data`, `color_space` `linear`, `dimension` `2d` |
| `fill` | array of 4 integers | `[255, 255, 255, 255]` | each `0..=255`; only with `pack` |

- Input names follow the texture name rules and share their namespace. The
  same input may fill several channels and several packed textures.
- At most **16** textures.
- Usages `noise`, `gradient`, `palette`, `atlas` need `dimension = "2d"`;
  `environment` needs `"cube"`; `lut` takes `"2d"` (strip) or `"3d"`
  (volume).

## Written by assetc

The Shader tab rewrites the whole file: `doc`, `params` (inline), `[state]`,
`[stages]` (each only when it has a key set), then one `[[textures]]` per
texture, keys in the order of the tables above, `pack` inline with its
channels in `r`, `g`, `b`, `a` order. Only keys that are set are written;
blank line between tables; comments are not kept; floats as in
[`.amat`](amat.md#written-by-assetc).

## Example

```toml
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
bare = true

[[textures]]
name = "mask"
usage = "data"
pack = { r = "foam", g = "depth", b = "caustics.g" }
fill = [0, 255, 0, 255]

[[textures]]
name = "sky"
usage = "environment"
```
