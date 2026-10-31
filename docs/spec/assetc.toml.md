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

### `[textures]`

Per-texture settings; see [`docs/features/textures.md`](../features/textures.md)
for glob syntax and what each setting does.

| Key          | Type             | Default | Meaning                                           |
|--------------|------------------|---------|---------------------------------------------------|
| `linear`     | array of strings | `[]`    | Globs of textures holding linear data, not sRGB color. |
| `no_mipmaps` | array of strings | `[]`    | Globs of textures written without mipmaps.        |

Patterns are matched against the source path relative to `input`, with its
extension (`textures/rock_n.png`).

### Paths

`input` and `output` are relative to the project root (the directory containing
`assetc.toml`). Absolute paths are used as-is. Neither directory has to exist
when the config is loaded.

Unknown keys (at the top level or inside `[textures]`) are rejected with a parse error, so typos are caught early
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
```
