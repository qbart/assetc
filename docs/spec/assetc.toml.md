# `assetc.toml`

Project configuration file. Its presence marks the **project root**; see
[`docs/features/cli.md`](../features/cli.md) for how it is discovered.

## Encoding

- [TOML 1.1](https://toml.io/en/v1.1.0), UTF-8.
- File name is exactly `assetc.toml` (case-sensitive).

## Schema

The schema is currently empty: no keys are defined. An empty file, or a file
containing only comments, is a valid config.

Unknown keys are rejected with a parse error, so typos are caught early
instead of being silently ignored.

## File written by `assetc init`

```toml
# assetc project configuration.
# See docs/spec/assetc.toml.md for available options.
```
