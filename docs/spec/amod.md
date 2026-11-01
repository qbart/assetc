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

## Written form

assetc writes only the keys that are set, in the order listed, each table
preceded by a blank line except the first:

```toml
[mesh]
meshlet_max_vertices = 128
meshlet_max_triangles = 256
```

When nothing is left to write, assetc deletes the file instead.

## Reserved

`[[modifiers]]` is reserved for modifier stacks on input images (see
`DEFERRED.md`).
