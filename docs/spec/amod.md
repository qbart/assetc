# `.amod` sidecars

Per-file settings for one input file, kept next to it as
`<file name>.amod` (the full file name plus `.amod`, e.g. `Fox.glb.amod`
for `Fox.glb`). assetc reads them when it builds and inspects that file, and
writes them from the Inspector. An `.amod` file is not a source of its own
(the build counts it as skipped).

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8. Every table and key is
optional; an empty file means "no settings". Unknown tables or keys are
errors (the file it belongs to then fails to build, with
`<file>.amod: <problem>`).

## `[mesh]`

For glTF models (`*.gltf`, `*.glb`); see
[`docs/features/models.md`](../features/models.md#build).

Any key of [`assetc.toml`'s `[meshes]`](assetc.toml.md#meshes) except
`overrides`, with the same types and ranges (`meshlet_max_vertices`,
`meshlet_max_triangles`, `meshlet_cone_weight`, `lod_ratios`, `lod_error`,
`lod_error_step`, `lod_attempts`, `lod_sloppy_fallback`, `lod_uv_weight`,
`lod_normal_weight`, `lod_permissive`, `lod_lock_border`, `lod_regularize`,
`lod_prune`).

A key set here wins over [`assetc.toml`](assetc.toml.md#meshes) (its
defaults and `[[meshes.overrides]]`); a key left out keeps the project's
value.

## `[[locators]]`

For glTF models; see [`docs/features/locators.md`](../features/locators.md).
A list of locators, one table each:

| Key           | Type              | Default        | Meaning                                           |
|---------------|-------------------|----------------|---------------------------------------------------|
| `name`        | string            | (required)     | non-empty, unique among the file's locators       |
| `tags`        | array of strings  | `[]`           | each non-empty, without commas, at most once      |
| `joint`       | string            | (none)         | name of a joint of the model's first skin; left out: model space |
| `translation` | array of 3 floats | `[0, 0, 0]`    | in the parent's (joint's or model's) space        |
| `rotation`    | array of 4 floats | `[0, 0, 0, 1]` | quaternion `x y z w`, normalized when built; not all zero |
| `scale`       | array of 3 floats | `[1, 1, 1]`    | per axis                                          |
| `preview`     | string            | (none)         | input-relative path of a glTF model shown at the locator by the 3D view; not compiled |
| `color`       | string            | (none)         | `#rrggbb`, the marker's color in the 3D view; not compiled |

An empty or repeated `name`, a missing one, a wrong array length, an empty,
repeated or comma-holding tag, a `color` that is not `#` and six hex digits,
or an unknown key is an error. A locator named like one the model inherits (from
its file or a preset) replaces it, see
[Inherited locators](../features/locators.md#inherited-locators). Whether `joint` exists is checked when the model
builds.

## `[properties]`

For glTF models; see [`docs/features/properties.md`](../features/properties.md).
A table of the model's properties: each key is a property's key (any TOML
key; quote it when it is not a bare key), each value a boolean, an integer,
a float or a string (Bool, Int, Float, Text). Any other value (an array, a
table, a date) is an error.

## `[[shapes]]`

For glTF models; see [`docs/features/shapes.md`](../features/shapes.md).
One table per shape:

| Key           | Type              | Default        | Meaning |
|---------------|-------------------|----------------|---------|
| `name`        | string            | (required)     | non-empty, unique among the file's shapes |
| `kind`        | string            | `""`           | free text |
| `type`        | string            | (required)     | `box`, `sphere`, `capsule` or `hull` |
| `joint`       | string            | (none)         | as a locator's |
| `translation` | array of 3 floats | `[0, 0, 0]`    | not for `hull` |
| `rotation`    | array of 4 floats | `[0, 0, 0, 1]` | quaternion `x y z w`, not all zero; not for `hull` |
| `size`        | array of 3 floats | `[1, 1, 1]`    | `box` only; each > 0 |
| `radius`      | float             | `0.5`          | `sphere` and `capsule` only; > 0 |
| `height`      | float             | `1`            | `capsule` only; > 0 |
| `color`       | string            | (none)         | `#rrggbb`, not compiled |
| `properties`  | table             | (none)         | as [`[properties]`](#properties), written as a `[shapes.properties]` table after the shape's keys |

A key that does not apply to the shape's type is an error, like an unknown
key.

## `[[regions]]`

For glTF models; see [`docs/features/regions.md`](../features/regions.md).
One table per region, at most 32:

| Key         | Type                      | Default    | Meaning |
|-------------|---------------------------|------------|---------|
| `name`      | string                    | (required) | non-empty, unique among the file's regions |
| `tags`      | array of strings          | `[]`       | as a locator's |
| `color`     | string                    | (required) | `#rrggbb`, not compiled |
| `materials` | array of strings          | `[]`       | glTF material names, each at most once |
| `submeshes` | array of arrays of 2 integers | `[]`   | whole glTF primitives as `[mesh, primitive]`, each at most once |
| `triangles` | array of arrays of 4 integers | `[]`   | painted triangles as runs `[mesh, primitive, first, count]`: triangles `first` to `first + count − 1` of primitive `primitive` of glTF mesh `mesh`; `count` ≥ 1 |

assetc writes `submeshes` sorted by mesh and primitive, and `triangles`
sorted by mesh, primitive and triangle, with adjacent and overlapping runs
merged, four runs per line.

## `[[modifiers]]`

For images (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`); see
[`docs/features/textures.md`](../features/textures.md#modifiers). A list of
modifiers, exactly as in a `.noise` file: each table has `type`, `enabled`
and the modifier's parameters, see
[`docs/spec/noise.md`](noise.md#modifiers). They run in order on the image
whenever assetc loads it.

## Written form

assetc writes the `[mesh]` keys that are set, in the order listed, then
`[properties]` (sorted by key) when there are any, then each
locator as a `[[locators]]` table (`name`, `tags` when there are any,
`joint` when set, `translation`,
`rotation`, `scale`, always all three, floats in Rust's shortest form,
`preview` and `color` when set), then each shape as a `[[shapes]]` table
(`name`, `kind` when not empty, `type`, `joint` when set, `translation`,
`rotation` and the sizes of its type, `color` when set, then its
`[shapes.properties]` when it has any), then each region as a `[[regions]]`
table (`name`, `tags` when there are any, `color`, `materials` and
`triangles` when there are any), then
each modifier as a `[[modifiers]]` table (`type`, `enabled`, then every
parameter, as `.noise` files write them), each table preceded by a blank line
except the first:

```toml
[mesh]
meshlet_max_vertices = 128
meshlet_max_triangles = 256

[properties]
mass = 80.0
playable = true

[[locators]]
name = "hand_r.weapon"
joint = "RightHand"
translation = [0.02, 0.08, 0.0]
rotation = [0.0, 0.0, 0.70710677, 0.70710677]
scale = [1.0, 1.0, 1.0]

[[shapes]]
name = "head"
kind = "hitbox"
type = "capsule"
joint = "Head"
translation = [0.0, 0.1, 0.0]
rotation = [0.0, 0.0, 0.0, 1.0]
radius = 0.1
height = 0.12

[shapes.properties]
damage = 2.0

[[regions]]
name = "armor"
tags = ["metal"]
color = "#3b82f6"
materials = ["Steel"]
submeshes = [[1, 0]]
triangles = [[0, 0, 120, 40], [0, 1, 0, 12]]

[[modifiers]]
type = "levels"
enabled = true
black = 0.1
white = 0.9
gamma = 1.0
```

When nothing is left to write, assetc deletes the file instead.
