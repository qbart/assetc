# `atlas.toml`

Settings of an [atlas](../features/atlases.md), inside its `*.atlas`
folder. TOML, UTF-8; every key optional, unknown keys are errors.

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

assetc writes every key, in this order, one per line:

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
