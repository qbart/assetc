# `.acollider` sidecars

A glTF model's [collider](../features/colliders.md) settings, kept next to
the model as `<model file name>.acollider` (the full file name plus
`.acollider`, e.g. `Fox.glb.acollider` for `Fox.glb`). Its existence is what
makes the build write the model's
[`P.acollider.bin`](acollider.bin.md). It is not a source of its own (the
build counts it as skipped).

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8, top-level keys only. Every key
is optional; an empty file means the defaults. Unknown keys, tables and
values of the wrong type or out of range are errors (the model then fails to
build, with `<file>.acollider: <problem>`).

| Key            | Type    | Default | Range       |
|----------------|---------|---------|-------------|
| `max_hulls`    | integer | `32`    | 1–256       |
| `max_vertices` | integer | `32`    | 4–255       |
| `error`        | float (an integer is taken as a float) | `0.01` | 0.001–0.5 |
| `resolution`   | integer | `64`    | 16–256      |

Their meaning is in
[`colliders.md`](../features/colliders.md#settings).

```toml
max_hulls = 16
error = 0.02
```

## Written by assetc

The Inspector's [Collider tab](../features/colliders.md#collider-tab) writes
the file: **Make collider** an empty file; a changed setting replaces its
key's value or adds the key at the end (the rest of the file, comments
included, is kept); **Use defaults** removes every key. The file ends with a
newline.

## Lifetime

Moved, renamed and deleted with its model (see
[`file-operations.md`](../features/file-operations.md#companions)).
