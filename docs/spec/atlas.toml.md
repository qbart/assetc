# `atlas.toml`

Settings of an [atlas](../features/atlases.md), inside its `*.atlas`
folder. TOML, UTF-8; every key optional, unknown keys are errors;
`[[structs]]` and `[values]` hold [attached structs](#structs).

| Key            | Type    | Default  | Range |
|----------------|---------|----------|-------|
| `packing`      | string  | `"grid"` | `"grid"` or `"pack"` |
| `cell_width`   | integer | `0`      | 0–4096 |
| `cell_height`  | integer | `0`      | 0–4096 |
| `columns`      | integer | `0`      | 0–1024 |
| `padding`      | integer | `2`      | 0–64 |
| `extrude`      | integer | `0`      | 0–`padding` |
| `trim`         | boolean | `false`  | |
| `power_of_two` | boolean | `false`  | |
| `max_size`     | integer | `4096`   | 1–16384 |

assetc writes every key, in this order, one per line, then the
[structs](#structs):

```toml
packing = "grid"
cell_width = 0
cell_height = 0
columns = 0
padding = 2
extrude = 0
trim = false
power_of_two = false
max_size = 4096
```

## Structs

[Attached structs](../features/structs.md) of the atlas:

- **`[[structs]]`**: one table per attachment, in order: `schema` (the
  schema's path relative to this file's folder, forward slashes) and `type`
  (the struct's name), both strings; any other key is an error.
- **`[values.<Type>]`**: the atlas's values of the attached struct `<Type>`,
  one key per field it sets, sparse and normalised as an
  [instance's](adata.md#written-form), each value in
  [TOML form](../features/structs.md#values-in-toml) (struct values and
  lists inline, `null` as `{ "$null" = true }`). A struct with nothing set
  has no table.

assetc writes the `[[structs]]` tables, then the `[values.<Type>]` tables in
attachment order (struct values inline), each after a blank line:

```toml
[[structs]]
schema = "game.aschema"
type = "Pickup"

[values.Pickup]
mass = 2.5
tags = ["loot", "rare"]
stats = { hp = 40, armor = 2 }
```
