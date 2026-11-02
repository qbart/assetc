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

| Key                     | Type            | Default         | Meaning                                                    |
|-------------------------|-----------------|-----------------|------------------------------------------------------------|
| `meshlet_max_vertices`  | integer         | `64`            | Most vertices per meshlet, 3–256                           |
| `meshlet_max_triangles` | integer         | `124`           | Most triangles per meshlet, 4–512 and a multiple of 4      |
| `meshlet_cone_weight`   | float           | `0.25`          | 0–1: how much meshlet building favours tight normal cones (better back-face culling) over compact meshlets |
| `lod_ratios`            | array of floats | `[0.5, 0.25]`   | Reduced levels of detail, as fractions of the full triangle count; each in 0–1 (exclusive), decreasing, at most 8; `[]` writes no levels |
| `lod_error`             | float           | `0.05`          | 0–1: first error limit of simplification, relative to the mesh's size |
| `lod_error_step`        | float           | `0.05`          | 0–1: how much the limit rises per retry                    |
| `lod_attempts`          | integer         | `3`             | 0–10: retries with a raised limit while a level makes no progress |
| `lod_sloppy_fallback`   | boolean         | `true`          | After the retries, fall back to sloppy (topology-ignoring) simplification; otherwise the level reuses the previous one |
| `lod_uv_weight`         | float           | `1.0`           | 0–100: weight of UV error in simplification                 |
| `lod_normal_weight`     | float           | `0.5`           | 0–100: weight of normal error in simplification             |
| `lod_permissive`        | boolean         | `true`          | Allow collapses across attribute seams (UV / normal discontinuities) |
| `lod_lock_border`       | boolean         | `false`         | Keep vertices on the mesh's open borders in place           |
| `lod_regularize`        | boolean         | `false`         | Prefer evenly sized triangles, at some cost to accuracy     |
| `lod_prune`             | boolean         | `false`         | Remove small disconnected parts while simplifying           |
| `overrides`             | array of tables | `[]`        | Per-model values, see below                                |

These are the [meshoptimizer](https://github.com/zeux/meshoptimizer) settings
of the [build](../features/models.md#geometry): meshlets come from
`meshopt_buildMeshlets`, levels of detail from `meshopt_simplifyWithAttributes`
(the `lod_permissive`, `lod_lock_border`, `lod_regularize` and `lod_prune`
switches are its options) and `meshopt_simplifySloppy`.

Each `[[meshes.overrides]]` entry:

| Key                     | Type             | Meaning                                              |
|-------------------------|------------------|------------------------------------------------------|
| `match`                 | array of strings | Required. Globs matched against the model's input-relative path with its extension (`models/hero.glb`), with the syntax of `[textures]` |
| any key of `[meshes]` except `overrides` | as above | Optional, replaces the default for matching models |

For each setting, the **last** entry that matches the model and sets it wins;
otherwise the default applies. A model's own [`.amod`](amod.md#mesh) sidecar
wins over both.

```toml
[meshes]
meshlet_max_vertices = 64
meshlet_max_triangles = 124

[[meshes.overrides]]
match = ["characters/**"]
meshlet_max_vertices = 128
meshlet_max_triangles = 256
```

The meshlet limits must suit the mesh shaders that draw the meshlets: one
meshlet is one workgroup's output, so they are a per-GPU tuning choice. A
value outside its range (in the defaults or any override) or a bad glob fails
the build before anything is compiled, with `assetc.toml [meshes]: <problem>`.

### `[locators]`

Locators shared between models, see
[`docs/features/locators.md`](../features/locators.md#presets).

| Key       | Type            | Default | Meaning                       |
|-----------|-----------------|---------|-------------------------------|
| `presets` | array of tables | `[]`    | Shared locators, see below    |

Each `[[locators.presets]]` entry:

| Key     | Type             | Meaning |
|---------|------------------|---------|
| `from`  | string           | Required. Input-relative path of the glTF model whose locators (from its file and its sidecar) are shared |
| `match` | array of strings | Optional. Globs of the models it applies to (syntax of `[textures]`); every model when left out. Only models whose first skin has every joint the locators use get them |

```toml
[[locators.presets]]
from = "characters/hero.glb"
match = ["characters/**"]
```

### Paths

`input` and `output` are relative to the project root (the directory containing
`assetc.toml`). Absolute paths are used as-is. Neither directory has to exist
when the config is loaded.

Unknown keys (at the top level, inside `[textures]`, `[meshes]`,
`[locators]`, an override or a preset) are rejected with a parse error, so typos are caught early
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
