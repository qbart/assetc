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
| `joint`       | string            | (none)         | name of a joint of the model's first skin; left out: model space |
| `translation` | array of 3 floats | `[0, 0, 0]`    | in the parent's (joint's or model's) space        |
| `rotation`    | array of 4 floats | `[0, 0, 0, 1]` | quaternion `x y z w`, normalized when built; not all zero |
| `scale`       | array of 3 floats | `[1, 1, 1]`    | per axis                                          |
| `preview`     | string            | (none)         | input-relative path of a glTF model shown at the locator by the 3D view; not compiled |

An empty or repeated `name`, a missing one, a wrong array length or an
unknown key is an error. Whether `joint` exists is checked when the model
builds.

## `[[modifiers]]`

For images (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`); see
[`docs/features/textures.md`](../features/textures.md#modifiers). A list of
modifiers, exactly as in a `.noise` file: each table has `type`, `enabled`
and the modifier's parameters, see
[`docs/spec/noise.md`](noise.md#modifiers). They run in order on the image
whenever assetc loads it.

## Written form

assetc writes the `[mesh]` keys that are set, in the order listed, then each
locator as a `[[locators]]` table (`name`, `joint` when set, `translation`,
`rotation`, `scale`, always all three, floats in Rust's shortest form,
`preview` when set), then
each modifier as a `[[modifiers]]` table (`type`, `enabled`, then every
parameter, as `.noise` files write them), each table preceded by a blank line
except the first:

```toml
[mesh]
meshlet_max_vertices = 128
meshlet_max_triangles = 256

[[locators]]
name = "hand_r.weapon"
joint = "RightHand"
translation = [0.02, 0.08, 0.0]
rotation = [0.0, 0.0, 0.70710677, 0.70710677]
scale = [1.0, 1.0, 1.0]

[[modifiers]]
type = "levels"
enabled = true
black = 0.1
white = 0.9
gamma = 1.0
```

When nothing is left to write, assetc deletes the file instead.
