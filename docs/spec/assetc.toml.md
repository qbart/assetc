# `assetc.toml`

Project configuration file. Its presence marks the **project root**; see
[`docs/features/cli.md`](../features/cli.md) for how it is discovered.

## Encoding

- [TOML 1.1](https://toml.io/en/v1.1.0), UTF-8.
- File name is exactly `assetc.toml` (case-sensitive).

## Schema

All keys are optional. An empty file, or a file containing only comments, is
a valid config and uses the defaults below.

| Key      | Type   | Default    | Meaning                                         |
|----------|--------|------------|-------------------------------------------------|
| `input`  | string | `"assets"` | Directory holding source assets.                |
| `output` | string | `"build"`  | Directory compiled assets are written to.       |
| `textures` | table | empty     | Per-texture settings, see below.                |
| `meshes` | table   | see below  | Mesh build settings, see below.                 |

### `[textures]`

Per-texture settings; see [`docs/features/textures.md`](../features/textures.md)
for glob syntax and what each setting does.

| Key          | Type             | Default | Meaning                                           |
|--------------|------------------|---------|---------------------------------------------------|
| `linear`     | array of strings | `[]`    | Globs of textures holding linear data, not sRGB color. |
| `no_mipmaps` | array of strings | `[]`    | Globs of textures written without mipmaps.        |
| `volume_luts` | array of strings | `[]`   | Globs of color LUTs (`*.cube`) compiled as 3D textures instead of 2D strips, see [`lut.md`](../features/lut.md#build). |

Patterns are matched against the source path relative to `input`, with its
extension (`textures/rock_n.png`).

### `[meshes]`

Settings for meshes compiled from glTF models (see
[`docs/features/models.md`](../features/models.md#build)): project-wide
defaults, overridden per model by `[[meshes.overrides]]` entries.

| Key                     | Type    | Default | Meaning                                              |
|-------------------------|---------|---------|------------------------------------------------------|
| `meshlet_max_vertices`  | integer | `64`    | Most vertices per meshlet, 3–256                     |
| `meshlet_max_triangles` | integer | `124`   | Most triangles per meshlet, 4–512 and a multiple of 4 |
| `overrides`             | array of tables | `[]` | Per-model values, see below                    |

Each `[[meshes.overrides]]` entry:

| Key                     | Type             | Meaning                                              |
|-------------------------|------------------|------------------------------------------------------|
| `match`                 | array of strings | Required. Globs matched against the model's input-relative path with its extension (`models/hero.glb`), with the syntax of `[textures]` |
| `meshlet_max_vertices`  | integer          | Optional, replaces the default for matching models   |
| `meshlet_max_triangles` | integer          | Optional, replaces the default for matching models   |

For each setting, the **last** entry that matches the model and sets it wins;
otherwise the default applies.

```toml
[meshes]
meshlet_max_vertices = 64
meshlet_max_triangles = 124

[[meshes.overrides]]
match = ["characters/**"]
meshlet_max_vertices = 128
meshlet_max_triangles = 256
```

The limits must suit the mesh shaders that draw the meshlets: one meshlet is
one workgroup's output, so they are a per-GPU tuning choice. A value outside
its range (in the defaults or any override) or a bad glob fails the build
before anything is compiled, with `assetc.toml [meshes]: <problem>`.

### Paths

`input` and `output` are relative to the project root (the directory containing
`assetc.toml`). Absolute paths are used as-is. Neither directory has to exist
when the config is loaded.

Unknown keys (at the top level, inside `[textures]`, `[meshes]` or an
override) are rejected with a parse error, so typos are caught early
instead of being silently ignored.

## File written by `assetc init`

```toml
# assetc project configuration.
# See docs/spec/assetc.toml.md for available options.

input = "assets"
output = "build"

# [textures]
# linear = ["**/*_n.png"]
# no_mipmaps = ["ui/**"]

# [meshes]
# meshlet_max_vertices = 64
# meshlet_max_triangles = 124
#
# [[meshes.overrides]]
# match = ["characters/**"]
# meshlet_max_vertices = 128
# meshlet_max_triangles = 256
```
