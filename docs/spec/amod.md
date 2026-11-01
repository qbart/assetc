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

## `[[modifiers]]`

For images (`*.png`, `*.jpg`, `*.jpeg`); see
[`docs/features/textures.md`](../features/textures.md#modifiers). A list of
modifiers, exactly as in a `.noise` file: each table has `type`, `enabled`
and the modifier's parameters, see
[`docs/spec/noise.md`](noise.md#modifiers). They run in order on the image
whenever assetc loads it.

## Written form

assetc writes the `[mesh]` keys that are set, in the order listed, then each
modifier as a `[[modifiers]]` table (`type`, `enabled`, then every parameter,
as `.noise` files write them), each table preceded by a blank line except
the first:

```toml
[mesh]
meshlet_max_vertices = 128
meshlet_max_triangles = 256

[[modifiers]]
type = "levels"
enabled = true
black = 0.1
white = 0.9
gamma = 1.0
```

When nothing is left to write, assetc deletes the file instead.
